/*
 * Copyright (c) 2020 Nanjing Xiaoxiongpai Intelligent Technology Co., Ltd.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *    http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#include "sdk_cxx.h"

#include "E53_SC1.h"
#include "wifi_connect.h"

/* ===== 联网配置:真实凭据在 include/app_config.h(.gitignore 忽略) ===== */
#include "app_config.h"

namespace {

/* 注意:IoTDA 8883 MQTTS 在本工程 iot_link/mbedtls 上实测不可用
 * (证书解析 calloc 内核崩溃、订阅 90s 超时、断开清理 panic,详见 git 记录),
 * 故设备侧保持 1883 明文。iotda_ca.h 保留备用,勿直接启用。
 * 实例设备侧域名在 include/app_config.h 的 CONFIG_APP_SERVERIP 配置。 */
constexpr char kServerPort[] = "1883"; // MQTT 明文(8883 TLS 在 Hi3861 iot_link 上不可用)
constexpr int kMqttLifeTimeSec = 60;   ///< 心跳周期,秒

constexpr int kQueueCapacity = 10;          // 消息队列深度
constexpr unsigned kCmdQueueTimeoutMs = 10; // 下行回调里 push 的超时
constexpr unsigned kReportQueueTimeoutMs = 5 * 1000; // 上报 push 的超时

constexpr uint32_t kSensorSampleUs = 50000; // 50ms 采样周期
constexpr int kReportIntervalTicks = 100;   // 5s = 100 x 50ms

/* auto 模式防闪烁(施密特触发 + 灯光自照度补偿 + 连续确认) */
constexpr float kLuxHysteresis = 40.0f; // 迟滞半带(lx),加大到 40 防止边界抖动
constexpr float kLampSelfLux = 30.0f;   // 补光灯满功率时在传感器处的自照度估计(lx)
constexpr int kSwitchConfirmTicks = 20; // 切换条件需连续满足的采样数(20 x 50ms = 1s)
constexpr int kLuxFilterSize = 5;       // 滑动平均窗口(5 × 50ms = 250ms)

/* PWM 调光:曲线最多 4 个锚点;目标亮度变化 ≥2% 才重配 PWM */
constexpr int kCurveMaxPoints = 4;
constexpr int kCurveLuxMax = 100000;
constexpr int kDimApplyMinDelta = 2;

enum class MsgType { Cmd, PropertySet, Report };
enum class ControlMode { Auto, Manual }; // 光照联动 / 手动控制(云端下发)

struct CmdMsg {
  char *request_id;
  char *payload;
};
struct ReportMsg {
  int lum;
};
struct AppMsg {
  MsgType type;
  union {
    CmdMsg cmd;
    ReportMsg report;
  } msg;
};

struct AppState {
  queue_t *queue; // 下行命令/属性设置 + 待上报消息队列
  int connected;  // MQTT 链路标志
  int led;        // 灯亮标志(输出亮度 > 0)
};
AppState g_app = {};

/* 任务间共享状态(主任务写 / 传感器任务读,单核 rv32 对齐字读写天然原子) */
volatile int g_threshold = 300; // 开关灯光照阈值,可被云端属性设置覆盖
volatile ControlMode g_mode = ControlMode::Auto;
volatile int g_applied = 0; // 当前实际输出亮度 %(灯态上报与自照度折算的依据)

/* 照度→亮度曲线:写方先改内容再置 len,读方先读 len 再读内容 */
struct CurvePoint {
  int lux;
  int pct;
};
CurvePoint g_curve[kCurveMaxPoints] = {};
volatile int g_curve_len = 0; // 0 = 未启用,回退施密特开关逻辑

/* SDK 结构体字段是非 const char*(C 头历史遗留),实际只读 */
inline char *sdk_str(const char *s) noexcept { return const_cast<char *>(s); }

/* malloc 缓冲的 RAII 封装:析构自动 free,release() 移交所有权给队列。
 * 手写而不用 std::unique_ptr:本工程 musl libc 与 g++ 自带 newlib
 * 头文件互斥,不引入任何 C++ 标准库头(也没有链接 -lstdc++)。 */
class MallocBuf {
public:
  explicit MallocBuf(void *p) : p_(p) {}
  ~MallocBuf() { free(p_); }
  MallocBuf(MallocBuf &&o) noexcept : p_(o.p_) { o.p_ = nullptr; }
  void *get() const { return p_; }
  void *release() {
    void *q = p_;
    p_ = nullptr;
    return q;
  }
  explicit operator bool() const { return p_ != nullptr; }
  MallocBuf(const MallocBuf &) = delete;
  MallocBuf &operator=(const MallocBuf &) = delete;

private:
  void *p_;
};

/* cJSON 的 RAII 封装:解析失败或作用域结束都自动释放,goto 链由此退役 */
class CJsonGuard {
public:
  explicit CJsonGuard(cJSON *p) : p_(p) {}
  ~CJsonGuard() {
    if (p_ != nullptr) {
      cJSON_Delete(p_);
    }
  }
  CJsonGuard(CJsonGuard &&o) noexcept : p_(o.p_) { o.p_ = nullptr; }
  cJSON *get() const { return p_; }
  explicit operator bool() const { return p_ != nullptr; }
  CJsonGuard(const CJsonGuard &) = delete;
  CJsonGuard &operator=(const CJsonGuard &) = delete;

private:
  cJSON *p_;
};

CJsonGuard parseJson(const char *s) { return CJsonGuard(cJSON_Parse(s)); }

/***************************************************************
 * 函数名称: BrightnessApply
 * 说    明: 应用输出亮度(0~100),同步灯态标志与输出记录
 ***************************************************************/
void BrightnessApply(int pct) {
  pct = pct < 0 ? 0 : (pct > 100 ? 100 : pct);
  Light_SetBrightness(pct);
  g_applied = pct;
  g_app.led = (pct > 0) ? 1 : 0;
}

/***************************************************************
 * 函数名称: LampSet
 * 说    明: 开关灯(ON 恒为满功率,与原行为一致)
 ***************************************************************/
void LampSet(int on) { BrightnessApply(on ? 100 : 0); }

/***************************************************************
 * 函数名称: DimCurveParse
 * 说    明: 解析 DimCurve 曲线串 "lux:pct,lux:pct,..."(≤4 点,
 *           lux 严格递增,0~100000;pct 0~100);空串 = 清空曲线,
 *           回退施密特开关。全程栈上解析零分配,合法才一次性提交
 * 返 回 值: true 接受,false 拒绝(旧配置不动)
 ***************************************************************/
bool DimCurveParse(const char *s) {
  if (s == nullptr) {
    return false;
  }
  if (*s == '\0') {
    g_curve_len = 0;
    return true;
  }
  CurvePoint pts[kCurveMaxPoints];
  int n = 0;
  const char *p = s;
  while (*p != '\0') {
    if (n >= kCurveMaxPoints) {
      return false;
    }
    char *end = nullptr;
    long lux = strtol(p, &end, 10);
    if (end == p || *end != ':') {
      return false;
    }
    p = end + 1;
    long pct = strtol(p, &end, 10);
    if (end == p || (*end != ',' && *end != '\0')) {
      return false;
    }
    if (lux < 0 || lux > kCurveLuxMax || pct < 0 || pct > 100) {
      return false;
    }
    if (n > 0 && lux <= pts[n - 1].lux) {
      return false;
    }
    pts[n] = CurvePoint{(int)lux, (int)pct};
    n++;
    p = end + (*end == ',' ? 1 : 0);
  }
  if (n == 0) {
    return false;
  }
  for (int i = 0; i < n; i++) {
    g_curve[i] = pts[i];
  }
  g_curve_len = n; // 最后置位:读方以 len 为准,不会读到半成品
  return true;
}

/***************************************************************
 * 函数名称: CurveEval
 * 说    明: 照度 → 目标亮度(分段线性插值;首点以下取首点,
 *           末点以上取末点,单点即恒定亮度)
 * 参    数: basis 扣除灯光自照度后的环境光照
 * 返 回 值: 目标亮度 %;曲线未启用返回 -1
 ***************************************************************/
int CurveEval(float basis) {
  int n = g_curve_len;
  if (n <= 0) {
    return -1;
  }
  if (basis <= (float)g_curve[0].lux) {
    return g_curve[0].pct;
  }
  if (basis >= (float)g_curve[n - 1].lux) {
    return g_curve[n - 1].pct;
  }
  for (int i = 1; i < n; i++) {
    if (basis < (float)g_curve[i].lux) {
      float t = (basis - (float)g_curve[i - 1].lux) /
                (float)(g_curve[i].lux - g_curve[i - 1].lux);
      float pct = (float)g_curve[i - 1].pct +
                  t * (float)(g_curve[i].pct - g_curve[i - 1].pct);
      return (int)(pct + 0.5f);
    }
  }
  return g_curve[n - 1].pct;
}

/***************************************************************
 * 函数名称: deal_report_msg
 * 说    明: 向 IoTDA 上报属性(光照值 + 灯态 + 当前亮度)
 ***************************************************************/
void deal_report_msg(ReportMsg *report) {
  if (g_app.connected != 1) {
    return;
  }
  int applied = g_applied;

  oc_mqtt_profile_service_t service{};
  oc_mqtt_profile_kv_t luminance{};
  oc_mqtt_profile_kv_t led{};
  oc_mqtt_profile_kv_t brightness{};

  service.event_time = nullptr;
  service.service_id = sdk_str("Light");
  service.service_property = &luminance;
  service.nxt = nullptr;

  luminance.key = sdk_str("Luminance");
  luminance.value = &report->lum;
  luminance.type = EN_OC_MQTT_PROFILE_VALUE_INT;
  luminance.nxt = &led;

  led.key = sdk_str("LightStatus");
  led.value = sdk_str(g_app.led ? "ON" : "OFF");
  led.type = EN_OC_MQTT_PROFILE_VALUE_STRING;
  led.nxt = &brightness;

  brightness.key = sdk_str("Brightness");
  brightness.value = &applied;
  brightness.type = EN_OC_MQTT_PROFILE_VALUE_INT;
  brightness.nxt = nullptr;

  /* 上报失败说明 MQTT 会话已死(网络抖过),清标志触发主循环重连 */
  if (oc_mqtt_profile_propertyreport(nullptr, &service) !=
      (int)en_oc_mqtt_err_ok) {
    g_app.connected = 0;
    printf("report failed, mqtt marked down\r\n");
  }
}

/***************************************************************
 * 函数名称: msg_rcv_callback
 * 说    明: oc_mqtt 下行消息回调,把命令/属性设置消息推入队列
 ***************************************************************/
extern "C" int msg_rcv_callback(oc_mqtt_profile_msgrcv_t *msg) {
  if (msg == nullptr || msg->request_id == nullptr ||
      (msg->type != EN_OC_MQTT_PROFILE_MSG_TYPE_DOWN_COMMANDS &&
       msg->type != EN_OC_MQTT_PROFILE_MSG_TYPE_DOWN_PROPERTYSET)) {
    return 0;
  }

  const size_t id_len = strlen(msg->request_id);
  const size_t buf_len =
      sizeof(AppMsg) + id_len + 1 + (size_t)msg->msg_len + 1;
  MallocBuf buf(malloc(buf_len));
  if (!buf) {
    return 0; // 内存不足静默丢弃(oc_mqtt 无重传语义,原有行为)
  }
  auto *app_msg = static_cast<AppMsg *>(buf.get());
  char *p = static_cast<char *>(buf.get()) + sizeof(AppMsg);

  app_msg->type = (msg->type == EN_OC_MQTT_PROFILE_MSG_TYPE_DOWN_COMMANDS)
                      ? MsgType::Cmd
                      : MsgType::PropertySet;
  app_msg->msg.cmd.request_id = p;
  memcpy(p, msg->request_id, id_len + 1);
  p += id_len + 1;
  app_msg->msg.cmd.payload = p;
  memcpy(p, msg->msg, (size_t)msg->msg_len);
  p[msg->msg_len] = '\0';

  int ret = queue_push(g_app.queue, app_msg, kCmdQueueTimeoutMs);
  if (ret == 0) {
    buf.release(); // 所有权移交队列,由消费方处理完自动回收
  }
  return ret;
}

/***************************************************************
 * 函数名称: deal_cmd_msg
 * 说    明: 处理平台命令 Light_Control_Led(ON/OFF/AUTO)
 ***************************************************************/
void deal_cmd_msg(CmdMsg *cmd) {
  int cmdret = 1;
  if (auto root = parseJson(cmd->payload)) {
    const cJSON *cmd_name = cJSON_GetObjectItem(root.get(), "command_name");
    if (cJSON_IsString(cmd_name) &&
        strcmp(cmd_name->valuestring, "Light_Control_Led") == 0) {
      const cJSON *paras = cJSON_GetObjectItem(root.get(), "paras");
      const cJSON *led =
          paras ? cJSON_GetObjectItem(paras, "Led") : nullptr;
      if (cJSON_IsString(led)) {
        if (strcmp(led->valuestring, "ON") == 0) {
          g_mode = ControlMode::Manual;
          LampSet(1);
          printf("Led On(manual)!\r\n");
        } else if (strcmp(led->valuestring, "OFF") == 0) {
          g_mode = ControlMode::Manual;
          LampSet(0);
          printf("Led Off(manual)!\r\n");
        } else if (strcmp(led->valuestring, "AUTO") == 0) {
          g_mode = ControlMode::Auto;
          printf("Back to auto mode!\r\n");
        }
        cmdret = 0;
      }
    }
  }
  oc_mqtt_profile_cmdresp_t cmdresp{};
  cmdresp.paras = nullptr;
  cmdresp.request_id = cmd->request_id;
  cmdresp.ret_code = cmdret;
  cmdresp.ret_name = nullptr;
  (void)oc_mqtt_profile_cmdresp(nullptr, &cmdresp);
}

/***************************************************************
 * 函数名称: deal_propertyset_msg
 * 说    明: 处理平台属性设置(Threshold 开关阈值 / Brightness
 *           手动亮度(设值即 manual) / DimCurve 照度-亮度曲线)
 ***************************************************************/
void deal_propertyset_msg(CmdMsg *cmd) {
  int ret = 1;
  if (auto root = parseJson(cmd->payload)) {
    const cJSON *services = cJSON_GetObjectItem(root.get(), "services");
    const cJSON *service =
        cJSON_IsArray(services) ? cJSON_GetArrayItem(services, 0) : nullptr;
    const cJSON *properties =
        service ? cJSON_GetObjectItem(service, "properties") : nullptr;
    if (properties != nullptr) {
      const cJSON *threshold = cJSON_GetObjectItem(properties, "Threshold");
      if (cJSON_IsNumber(threshold)) {
        g_threshold = (int)threshold->valuedouble;
        printf("threshold updated: %d\r\n", g_threshold);
        ret = 0;
      }
      const cJSON *brightness = cJSON_GetObjectItem(properties, "Brightness");
      if (cJSON_IsNumber(brightness)) {
        g_mode = ControlMode::Manual;
        BrightnessApply((int)brightness->valuedouble);
        printf("brightness set: %d%%(manual)\r\n", g_applied);
        ret = 0;
      }
      const cJSON *curve = cJSON_GetObjectItem(properties, "DimCurve");
      if (cJSON_IsString(curve)) {
        if (DimCurveParse(curve->valuestring)) {
          printf("dim curve updated, %d point(s)\r\n", g_curve_len);
          ret = 0;
        } else {
          printf("dim curve rejected: %s\r\n", curve->valuestring);
        }
      }
    }
  }
  oc_mqtt_profile_propertysetresp_t resp{};
  resp.ret_code = ret;
  resp.ret_description = nullptr;
  resp.request_id = cmd->request_id;
  (void)oc_mqtt_profile_propertysetresp(nullptr, &resp);
}

/***************************************************************
 * 函数名称: mqtt_connect
 * 说    明: 建立一次到 IoTDA 的 MQTT 会话,可重复调用
 * 返 回 值: 0 成功,其他失败
 ***************************************************************/
int mqtt_connect(void) {
  oc_mqtt_profile_connect_t connect_para{};

  connect_para.boostrap = 0;
  connect_para.device_id = sdk_str(CONFIG_APP_DEVICEID);
  connect_para.device_passwd = sdk_str(CONFIG_APP_DEVICEPWD);
  connect_para.server_addr = sdk_str(CONFIG_APP_SERVERIP);
  connect_para.server_port = sdk_str(kServerPort);
  connect_para.life_time = kMqttLifeTimeSec;
  connect_para.rcvfunc = msg_rcv_callback;
  connect_para.security.type = EN_DTLS_AL_SECURITY_TYPE_NONE;
  /* 重连前先把可能残留的会话清掉 */
  (void)oc_mqtt_profile_disconnect();
  return oc_mqtt_profile_connect(&connect_para);
}

/***************************************************************
 * 函数名称: task_main_entry
 * 说    明: 主任务,连接 Wi-Fi 与 IoTDA,处理消息队列;
 *           状态机式链路维护:Wi-Fi 断开(Hi3861 驱动会自动重关联)则
 *           标记 MQTT 下线,Wi-Fi 恢复后自动重连 MQTT,断网不再变砖
 ***************************************************************/
extern "C" void task_main_entry(void *) {
  /* 队列创建失败不带 nullptr 进主循环(原 C 版会随后 queue_pop 崩溃) */
  while ((g_app.queue = queue_create("queue_rcvmsg", kQueueCapacity, 1)) ==
         nullptr) {
    printf("create msg queue failed, retry in 1s\r\n");
    osDelay(1000);
  }
  dtls_al_init();
  mqtt_al_init();
  oc_mqtt_init();

  /* 开机先连上 Wi-Fi(失败由 WifiConnect 返回 -1,这里退避重试) */
  while (WifiConnect(CONFIG_WIFI_SSID, CONFIG_WIFI_PWD) != 0) {
    printf("wifi connect failed, retry in 5s\r\n");
    osDelay(5000);
  }

  for (;;) {
    /* 链路健康检查:Wi-Fi 掉了 => MQTT 会话必然已死;
     * 等驱动重关联成功(WifiConnectStatus 回 1)后重连 MQTT */
    if (WifiConnectStatus() != 1) {
      g_app.connected = 0;
    }
    if (g_app.connected != 1) {
      if (WifiConnectStatus() != 1) {
        osDelay(1000);
        continue;
      }
      if (mqtt_connect() == (int)en_oc_mqtt_err_ok) {
        g_app.connected = 1;
        printf("oc_mqtt_profile_connect succed!\r\n");
      } else {
        printf("oc_mqtt_profile_connect faild, retry in 5s\r\n");
        osDelay(5000);
        continue;
      }
    }

    AppMsg *raw = nullptr;
    /* 1s 超时(原先是永久阻塞),让上面的健康检查周期性执行 */
    if (queue_pop(g_app.queue, reinterpret_cast<void **>(&raw), 1000) == 0 &&
        raw != nullptr) {
      MallocBuf msg(raw); // RAII:处理完自动 free
      switch (raw->type) {
      case MsgType::Cmd:
        deal_cmd_msg(&raw->msg.cmd);
        break;
      case MsgType::PropertySet:
        deal_propertyset_msg(&raw->msg.cmd);
        break;
      case MsgType::Report:
        deal_report_msg(&raw->msg.report);
        break;
      }
    }
  }
}

/***************************************************************
 * 函数名称: task_sensor_entry
 * 说    明: 光照采集任务,50ms 周期采样;auto 模式两条支路——
 *           配了 DimCurve 走照度-亮度曲线(|Δ|≥2% 才重配 PWM,
 *           负反馈自稳定),没配走施密特触发开关灯(迟滞带 +
 *           扣除灯光自照度 + 连续 1s 确认);每 5s 推一条上报
 ***************************************************************/
extern "C" void task_sensor_entry(void *) {
  E53_SC1_Init();
  usleep(20000); // 等待 BH1750 完成第一次转换(16ms)

  printf("=======================================\r\n");
  printf("********* smart street light **********\r\n");
  printf("===== L-res mode, 50ms sample =========\r\n");

  float lux_buf[kLuxFilterSize] = {}; // 滤波缓冲区
  int lux_buf_idx = 0;
  bool lux_buf_filled = false;
  int tick = 0;
  int confirm_ticks = 0;
  int auto_debug_tick = 0; // auto 模式调试输出计数

  for (;;) {
    float lux = E53_SC1_Read_Data();
    /* 滑动平均滤波:BH1750 在 L-res 模式下单次跳变可达 ±1000lx,
     * 用 5 点中值+均值混合滤波消除尖峰 */
    lux_buf[lux_buf_idx] = lux;
    lux_buf_idx = (lux_buf_idx + 1) % kLuxFilterSize;
    if (!lux_buf_filled && lux_buf_idx == 0) {
      lux_buf_filled = true;
    }
    {
      int count = lux_buf_filled ? kLuxFilterSize : lux_buf_idx;
      if (count > 0) {
        /* 排序取中值(冒泡,5 元素够快) */
        float sorted[kLuxFilterSize];
        memcpy(sorted, lux_buf, count * sizeof(float));
        for (int i = 0; i < count - 1; i++) {
          for (int j = 0; j < count - i - 1; j++) {
            if (sorted[j] > sorted[j + 1]) {
              float t = sorted[j];
              sorted[j] = sorted[j + 1];
              sorted[j + 1] = t;
            }
          }
        }
        float median = sorted[count / 2];
        /* 取中值和当前值的较小者,进一步抑制向上尖峰 */
        lux = (median < lux) ? median : (median * 0.7f + lux * 0.3f);
      }
    }
    if (g_mode == ControlMode::Auto) {
      if (g_curve_len > 0) {
        /* 曲线调光:自照度按实际占空比(γ 校正后)线性折算,灯越亮
         * basis 越高、目标亮度越低,负反馈自稳定,无开关式频闪 */
        float basis =
            lux - kLampSelfLux * (float)Light_DutyPercent(g_applied) / 100.0f;
        if (basis < 0) {
          basis = 0;
        }
        int target = CurveEval(basis);
        int delta = target - g_applied;
        if (delta >= kDimApplyMinDelta || delta <= -kDimApplyMinDelta) {
          BrightnessApply(target);
          printf("dim %d%%(auto curve, lux=%.1f basis=%.1f)\r\n", target,
                 lux, basis);
        }
        if (++auto_debug_tick >= 20) {
          auto_debug_tick = 0;
          printf("auto(curve): lux=%.1f basis=%.1f applied=%d\r\n", lux,
                 basis, g_applied);
        }
      } else {
        /* 直接 lux < threshold 翻转灯会频闪:补光灯照回传感器,开灯读数
         * 立刻越过阈值 -> 关灯 -> 掉回 -> 再开,形成 10Hz 自反馈振荡。
         * 这里灯亮时扣除自照度只按环境光判断,并加迟滞带与连续确认。 */
        float basis = lux;
        int want;
        if (g_app.led) {
          basis -= kLampSelfLux;
          if (basis < 0) {
            basis = 0;
          }
          want = (basis > (float)g_threshold + kLuxHysteresis) ? 0 : 1;
        } else {
          want = (basis < (float)g_threshold - kLuxHysteresis) ? 1 : 0;
        }
        if (want != g_app.led) {
          if (++confirm_ticks >= kSwitchConfirmTicks) {
            confirm_ticks = 0;
            LampSet(want);
            printf("lamp %s(auto, lux=%.1f basis=%.1f th=%d)\r\n",
                   want ? "ON" : "OFF", lux, basis, g_threshold);
          }
        } else {
          confirm_ticks = 0;
        }
        /* 每秒输出一次 auto 模式状态,方便观察持续监测 */
        if (++auto_debug_tick >= 20) {
          auto_debug_tick = 0;
          printf("auto: lux=%.1f th=%d led=%d want=%d confirm=%d\r\n", lux,
                 g_threshold, g_app.led, want, confirm_ticks);
        }
      }
    } else {
      confirm_ticks = 0;
    }

    if (++tick >= kReportIntervalTicks) {
      tick = 0;
      printf("Lux data:%.2f\r\n", lux);
      MallocBuf buf(malloc(sizeof(AppMsg)));
      if (buf) {
        auto *msg = static_cast<AppMsg *>(buf.get());
        msg->type = MsgType::Report;
        msg->msg.report.lum = (int)lux;
        if (queue_push(g_app.queue, msg, kReportQueueTimeoutMs) == 0) {
          buf.release(); // 所有权移交队列
        }
      }
    }
    usleep(kSensorSampleUs);
  }
}

/***************************************************************
 * 函数名称: OC_StreetLight
 * 说    明: 入口,创建主任务与光照采集任务
 ***************************************************************/
extern "C" void OC_StreetLight(void) {
  osThreadAttr_t attr{};

  attr.name = "task_main_entry";
  attr.attr_bits = 0U;
  attr.cb_mem = nullptr;
  attr.cb_size = 0U;
  attr.stack_mem = nullptr;
  attr.stack_size = 10240;
  attr.priority = static_cast<osPriority_t>(24);

  if (osThreadNew(task_main_entry, nullptr, &attr) == nullptr) {
    printf("Falied to create task_main_entry!\n");
  }
  attr.stack_size = 4096;
  attr.priority = static_cast<osPriority_t>(25);
  attr.name = "task_sensor_entry";
  if (osThreadNew(task_sensor_entry, nullptr, &attr) == nullptr) {
    printf("Falied to create task_sensor_entry!\n");
  }
}

} // namespace

APP_FEATURE_INIT(OC_StreetLight);
