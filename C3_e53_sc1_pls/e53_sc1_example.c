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

#include "ohos_init.h"
#include "cmsis_os2.h"

#include "wifi_connect.h"
#include <cJSON.h>
#include <queue.h>
#include <oc_mqtt_al.h>
#include <oc_mqtt_profile.h>
#include <dtls_al.h>
#include <mqtt_al.h>

#include "E53_SC1.h"

/* ===== 联网配置:真实凭据在 include/app_config.h(.gitignore 忽略) ===== */
#include "app_config.h"

#define CONFIG_APP_SERVERIP "69b5bf8bcd.st1.iotda-device.cn-south-1.myhuaweicloud.com" // IoTDA 实例设备侧域名
#define CONFIG_APP_SERVERPORT "1883"
#define CONFIG_APP_LIFETIME 60              ///< 心跳周期,秒

#define CONFIG_QUEUE_TIMEOUT (5 * 1000)
#define MSGQUEUE_OBJECTS 16

#define SENSOR_SAMPLE_US 50000              // 50ms 采样周期
#define REPORT_INTERVAL_TICKS 100           // 5s = 100 x 50ms

/* auto 模式防闪烁(施密特触发 + 灯光自照度补偿 + 连续确认) */
#define LUX_HYSTERESIS 20                   // 迟滞半带(lx),需 > 读数噪声(L-res 1 计数≈7lx)
#define LAMP_SELF_LUX 200                   // 补光灯在传感器处的自照度估计(lx),需实测校准
#define SWITCH_CONFIRM_TICKS 20             // 切换条件需连续满足的采样数(20 x 50ms = 1s)

#define MODE_AUTO 0   // 光照联动
#define MODE_MANUAL 1 // 手动控制(云端下发)

typedef enum {
  en_msg_cmd = 0,
  en_msg_propertyset,
  en_msg_report,
} en_msg_type_t;

typedef struct {
  char *request_id;
  char *payload;
} cmd_t;

typedef struct {
  int lum;
} report_t;

typedef struct {
  en_msg_type_t msg_type;
  union {
    cmd_t cmd;
    report_t report;
  } msg;
} app_msg_t;

typedef struct {
  queue_t *app_msg;
  int connected;
  int led;
} app_cb_t;
static app_cb_t g_app_cb;

/* 任务间共享状态 */
static volatile int g_threshold = 40; // 开关灯光照阈值,可被云端属性设置覆盖
static volatile int g_mode = MODE_AUTO; // 当前控制模式

/***************************************************************
 * 函数名称: LampSet
 * 说    明: 设置补光灯并记录当前灯态
 * 参    数: on 非 0 点亮,0 熄灭
 * 返 回 值: 无
 ***************************************************************/
static void LampSet(int on) {
  g_app_cb.led = on;
  Light_StatusSet(on ? ON : OFF);
}

/***************************************************************
 * 函数名称: deal_report_msg
 * 说    明: 向 IoTDA 上报属性(光照值 + 灯态)
 * 参    数: report 上报数据
 * 返 回 值: 无
 ***************************************************************/
static void deal_report_msg(report_t *report) {
  oc_mqtt_profile_service_t service;
  oc_mqtt_profile_kv_t luminance;
  oc_mqtt_profile_kv_t led;

  if (g_app_cb.connected != 1) {
    return;
  }

  service.event_time = NULL;
  service.service_id = "Light";
  service.service_property = &luminance;
  service.nxt = NULL;

  luminance.key = "Luminance";
  luminance.value = &report->lum;
  luminance.type = EN_OC_MQTT_PROFILE_VALUE_INT;
  luminance.nxt = &led;

  led.key = "LightStatus";
  led.value = g_app_cb.led ? "ON" : "OFF";
  led.type = EN_OC_MQTT_PROFILE_VALUE_STRING;
  led.nxt = NULL;

  oc_mqtt_profile_propertyreport(NULL, &service);
  return;
}

/***************************************************************
 * 函数名称: msg_rcv_callback
 * 说    明: oc_mqtt 下行消息回调,把命令/属性设置消息推入队列
 * 参    数: msg 下行消息
 * 返 回 值: 0 成功,其他失败
 ***************************************************************/
static int msg_rcv_callback(oc_mqtt_profile_msgrcv_t *msg) {
  int ret = 0;
  char *buf;
  int buf_len;
  app_msg_t *app_msg;

  if ((NULL == msg) || (msg->request_id == NULL) ||
      (msg->type != EN_OC_MQTT_PROFILE_MSG_TYPE_DOWN_COMMANDS &&
       msg->type != EN_OC_MQTT_PROFILE_MSG_TYPE_DOWN_PROPERTYSET)) {
    return ret;
  }

  buf_len = sizeof(app_msg_t) + strlen(msg->request_id) + 1 + msg->msg_len + 1;
  buf = malloc(buf_len);
  if (NULL == buf) {
    return ret;
  }
  app_msg = (app_msg_t *)buf;
  buf += sizeof(app_msg_t);

  app_msg->msg_type = (msg->type == EN_OC_MQTT_PROFILE_MSG_TYPE_DOWN_COMMANDS)
                          ? en_msg_cmd
                          : en_msg_propertyset;
  app_msg->msg.cmd.request_id = buf;
  buf_len = strlen(msg->request_id);
  buf += buf_len + 1;
  memcpy(app_msg->msg.cmd.request_id, msg->request_id, buf_len);
  app_msg->msg.cmd.request_id[buf_len] = '\0';

  buf_len = msg->msg_len;
  app_msg->msg.cmd.payload = buf;
  memcpy(app_msg->msg.cmd.payload, msg->msg, buf_len);
  app_msg->msg.cmd.payload[buf_len] = '\0';

  ret = queue_push(g_app_cb.app_msg, app_msg, 10);
  if (ret != 0) {
    free(app_msg);
  }

  return ret;
}

/***************************************************************
 * 函数名称: deal_cmd_msg
 * 说    明: 处理平台命令 Light_Control_Led(ON/OFF/AUTO)
 * 参    数: cmd 命令消息
 * 返 回 值: 无
 ***************************************************************/
static void deal_cmd_msg(cmd_t *cmd) {
  cJSON *obj_root;
  cJSON *obj_cmdname;
  cJSON *obj_paras;
  cJSON *obj_para;

  int cmdret = 1;
  oc_mqtt_profile_cmdresp_t cmdresp;
  obj_root = cJSON_Parse(cmd->payload);
  if (NULL == obj_root) {
    goto EXIT_JSONPARSE;
  }

  obj_cmdname = cJSON_GetObjectItem(obj_root, "command_name");
  if (NULL == obj_cmdname) {
    goto EXIT_CMDOBJ;
  }
  if (0 == strcmp(cJSON_GetStringValue(obj_cmdname), "Light_Control_Led")) {
    obj_paras = cJSON_GetObjectItem(obj_root, "paras");
    if (NULL == obj_paras) {
      goto EXIT_OBJPARAS;
    }
    obj_para = cJSON_GetObjectItem(obj_paras, "Led");
    if (NULL == obj_para) {
      goto EXIT_OBJPARA;
    }
    if (0 == strcmp(cJSON_GetStringValue(obj_para), "ON")) {
      g_mode = MODE_MANUAL;
      LampSet(1);
      printf("Led On(manual)!\r\n");
    } else if (0 == strcmp(cJSON_GetStringValue(obj_para), "OFF")) {
      g_mode = MODE_MANUAL;
      LampSet(0);
      printf("Led Off(manual)!\r\n");
    } else if (0 == strcmp(cJSON_GetStringValue(obj_para), "AUTO")) {
      g_mode = MODE_AUTO;
      printf("Back to auto mode!\r\n");
    }
    cmdret = 0;
  }

EXIT_OBJPARA:
EXIT_OBJPARAS:
EXIT_CMDOBJ:
  cJSON_Delete(obj_root);
EXIT_JSONPARSE:
  cmdresp.paras = NULL;
  cmdresp.request_id = cmd->request_id;
  cmdresp.ret_code = cmdret;
  cmdresp.ret_name = NULL;
  (void)oc_mqtt_profile_cmdresp(NULL, &cmdresp);
  return;
}

/***************************************************************
 * 函数名称: deal_propertyset_msg
 * 说    明: 处理平台属性设置(Threshold),更新光照阈值
 * 参    数: cmd 属性设置消息
 * 返 回 值: 无
 ***************************************************************/
static void deal_propertyset_msg(cmd_t *cmd) {
  cJSON *obj_root;
  cJSON *obj_services;
  cJSON *obj_service;
  cJSON *obj_properties;
  cJSON *obj_threshold;

  int ret = 1;
  oc_mqtt_profile_propertysetresp_t resp;
  obj_root = cJSON_Parse(cmd->payload);
  if (NULL == obj_root) {
    goto EXIT_JSONPARSE;
  }

  obj_services = cJSON_GetObjectItem(obj_root, "services");
  if (!cJSON_IsArray(obj_services)) {
    goto EXIT_CMDOBJ;
  }
  obj_service = cJSON_GetArrayItem(obj_services, 0);
  if (NULL == obj_service) {
    goto EXIT_CMDOBJ;
  }
  obj_properties = cJSON_GetObjectItem(obj_service, "properties");
  if (NULL == obj_properties) {
    goto EXIT_CMDOBJ;
  }
  obj_threshold = cJSON_GetObjectItem(obj_properties, "Threshold");
  if (cJSON_IsNumber(obj_threshold)) {
    g_threshold = (int)obj_threshold->valuedouble;
    printf("threshold updated: %d\r\n", g_threshold);
    ret = 0;
  }

EXIT_CMDOBJ:
  cJSON_Delete(obj_root);
EXIT_JSONPARSE:
  resp.ret_code = ret;
  resp.ret_description = NULL;
  resp.request_id = cmd->request_id;
  (void)oc_mqtt_profile_propertysetresp(NULL, &resp);
  return;
}

/***************************************************************
 * 函数名称: task_main_entry
 * 说    明: 主任务,连接 Wi-Fi 与 IoTDA,处理消息队列
 * 参    数: 无
 * 返 回 值: 0
 ***************************************************************/
static int task_main_entry(void) {
  app_msg_t *app_msg;
  uint32_t ret;

  WifiConnect(CONFIG_WIFI_SSID, CONFIG_WIFI_PWD);
  dtls_al_init();
  mqtt_al_init();
  oc_mqtt_init();

  g_app_cb.app_msg = queue_create("queue_rcvmsg", 10, 1);
  if (NULL == g_app_cb.app_msg) {
    printf("Create receive msg queue failed");
  }
  oc_mqtt_profile_connect_t connect_para;
  (void)memset(&connect_para, 0, sizeof(connect_para));

  connect_para.boostrap = 0;
  connect_para.device_id = CONFIG_APP_DEVICEID;
  connect_para.device_passwd = CONFIG_APP_DEVICEPWD;
  connect_para.server_addr = CONFIG_APP_SERVERIP;
  connect_para.server_port = CONFIG_APP_SERVERPORT;
  connect_para.life_time = CONFIG_APP_LIFETIME;
  connect_para.rcvfunc = msg_rcv_callback;
  connect_para.security.type = EN_DTLS_AL_SECURITY_TYPE_NONE;
  ret = oc_mqtt_profile_connect(&connect_para);
  if ((ret == (int)en_oc_mqtt_err_ok)) {
    g_app_cb.connected = 1;
    printf("oc_mqtt_profile_connect succed!\r\n");
  } else {
    printf("oc_mqtt_profile_connect faild!\r\n");
  }

  while (1) {
    app_msg = NULL;
    (void)queue_pop(g_app_cb.app_msg, (void **)&app_msg, 0xFFFFFFFF);
    if (NULL != app_msg) {
      switch (app_msg->msg_type) {
        case en_msg_cmd:
          deal_cmd_msg(&app_msg->msg.cmd);
          break;
        case en_msg_propertyset:
          deal_propertyset_msg(&app_msg->msg.cmd);
          break;
        case en_msg_report:
          deal_report_msg(&app_msg->msg.report);
          break;
        default:
          break;
      }
      free(app_msg);
    }
  }
  return 0;
}

/***************************************************************
 * 函数名称: task_sensor_entry
 * 说    明: 光照采集任务,50ms 周期采样;auto 模式施密特触发开关灯
 *           (迟滞带 + 扣除灯光自照度 + 连续 1s 确认,防阈值附近频闪);
 *           每 5s 推一条上报消息到队列
 * 参    数: 无
 * 返 回 值: 0
 ***************************************************************/
static int task_sensor_entry(void) {
  app_msg_t *app_msg;
  float lux;
  int tick = 0;
  int confirm_ticks = 0;

  E53_SC1_Init();
  usleep(20000); // 等待 BH1750 完成第一次转换(16ms)

  printf("=======================================\r\n");
  printf("********* smart street light **********\r\n");
  printf("===== L-res mode, 50ms sample =========\r\n");

  while (1) {
    lux = E53_SC1_Read_Data();
    if (g_mode == MODE_AUTO) {
      /* 直接 lux < threshold 翻转灯会频闪:补光灯照回传感器,开灯读数
       * 立刻越过阈值 -> 关灯 -> 掉回 -> 再开,形成 10Hz 自反馈振荡。
       * 这里灯亮时扣除自照度只按环境光判断,并加迟滞带与连续确认。 */
      float basis = lux;
      int want;
      if (g_app_cb.led) {
        basis -= LAMP_SELF_LUX;
        if (basis < 0) {
          basis = 0;
        }
        want = (basis > (float)g_threshold + LUX_HYSTERESIS) ? 0 : 1;
      } else {
        want = (basis < (float)g_threshold - LUX_HYSTERESIS) ? 1 : 0;
      }
      if (want != g_app_cb.led) {
        if (++confirm_ticks >= SWITCH_CONFIRM_TICKS) {
          confirm_ticks = 0;
          LampSet(want);
          printf("lamp %s(auto, lux=%.1f basis=%.1f th=%d)\r\n",
                 want ? "ON" : "OFF", lux, basis, g_threshold);
        }
      } else {
        confirm_ticks = 0;
      }
    } else {
      confirm_ticks = 0;
    }

    if (++tick >= REPORT_INTERVAL_TICKS) {
      tick = 0;
      printf("Lux data:%.2f\r\n", lux);
      app_msg = malloc(sizeof(app_msg_t));
      if (NULL != app_msg) {
        app_msg->msg_type = en_msg_report;
        app_msg->msg.report.lum = (int)lux;
        if (0 != queue_push(g_app_cb.app_msg, app_msg, CONFIG_QUEUE_TIMEOUT)) {
          free(app_msg);
        }
      }
    }
    usleep(SENSOR_SAMPLE_US);
  }
  return 0;
}

/***************************************************************
 * 函数名称: OC_StreetLight
 * 说    明: 入口,创建主任务与光照采集任务
 * 参    数: 无
 * 返 回 值: 无
 ***************************************************************/
static void OC_StreetLight(void) {
  osThreadAttr_t attr;

  attr.name = "task_main_entry";
  attr.attr_bits = 0U;
  attr.cb_mem = NULL;
  attr.cb_size = 0U;
  attr.stack_mem = NULL;
  attr.stack_size = 10240;
  attr.priority = 24;

  if (osThreadNew((osThreadFunc_t)task_main_entry, NULL, &attr) == NULL) {
    printf("Falied to create task_main_entry!\n");
  }
  attr.stack_size = 4096;
  attr.priority = 25;
  attr.name = "task_sensor_entry";
  if (osThreadNew((osThreadFunc_t)task_sensor_entry, NULL, &attr) == NULL) {
    printf("Falied to create task_sensor_entry!\n");
  }
}

APP_FEATURE_INIT(OC_StreetLight);
