/*
 * Copyright (c) 2020 Nanjing Xiaoxiongpai Intelligent Technology Co., Ltd.
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

#include "E53_SC1.h"
#include "sdk_cxx.h"

namespace {
/***************************************************************
 * 函数名称: E53_SC1_IO_Init
 * 说    明: E53_SC1_GPIO初始化
 ***************************************************************/
void E53_SC1_IO_Init(void) {
  GpioInit();
  IoSetFunc(WIFI_IOT_IO_NAME_GPIO_7,
            WIFI_IOT_IO_FUNC_GPIO_7_GPIO); // 设置GPIO_7的复用功能为普通GPIO
  GpioSetDir(WIFI_IOT_GPIO_IDX_7,
             WIFI_IOT_GPIO_DIR_OUT); // 设置GPIO_7为输出模式

  IoSetFunc(WIFI_IOT_IO_NAME_GPIO_0,
            WIFI_IOT_IO_FUNC_GPIO_0_I2C1_SDA); // GPIO_0复用为I2C1_SDA
  IoSetFunc(WIFI_IOT_IO_NAME_GPIO_1,
            WIFI_IOT_IO_FUNC_GPIO_1_I2C1_SCL); // GPIO_1复用为I2C1_SCL
  I2cInit(WIFI_IOT_I2C_IDX_1, 400000);         /* baudrate: 400kbps */
  I2cSetBaudrate(WIFI_IOT_I2C_IDX_1, 400000);
}

/* PWM 调光:GPIO7 复用为 PWM0_OUT,载波 160MHz/16000 = 10kHz */
constexpr unsigned short kPwmCarrierFreq = 16000;
bool g_pwm0_inited = false;

/* γ=2.2 感知亮度 → 占空比查找表(duty 计数 0~16000,.rodata 202B)。
 * 人眼感知是非线性的(Weber-Fechner/CIE L*):线性占空比的 50% 看着像 80%,
 * 10% 像 46%;经 γ=2.2 校正后 percent 才是感知意义上的"百分之几"。 */
constexpr uint16_t kGammaDuty[101] = {
      0,     1,     3,     7,    13,    22,    33,
     46,    62,    80,   101,   125,   151,   180,
    212,   246,   284,   324,   368,   414,   464,
    516,   572,   631,   693,   758,   826,   898,
    972,  1050,  1132,  1217,  1305,  1396,  1491,
   1589,  1690,  1795,  1904,  2016,  2131,  2250,
   2373,  2499,  2629,  2762,  2899,  3039,  3183,
   3331,  3482,  3637,  3796,  3958,  4125,  4295,
   4468,  4646,  4827,  5012,  5201,  5393,  5590,
   5790,  5994,  6202,  6414,  6630,  6849,  7073,
   7300,  7532,  7767,  8006,  8250,  8497,  8748,
   9003,  9262,  9526,  9793, 10064, 10340, 10619,
  10903, 11190, 11482, 11778, 12078, 12382, 12690,
  13002, 13318, 13639, 13964, 14293, 14626, 14963,
  15304, 15650, 16000
};
static_assert(sizeof(kGammaDuty) / sizeof(kGammaDuty[0]) == 101,
              "gamma table must have 101 entries");
} // namespace

/***************************************************************
 * 函数名称: Init_BH1750
 * 说    明: 写命令初始化BH1750(上电 + 进入连续低分辨率模式)
 ***************************************************************/
void Init_BH1750(void) {
  WifiIotI2cData bh1750_i2c_data{};
  uint8_t send_data[1] = {0x01}; // Power On
  bh1750_i2c_data.sendBuf = send_data;
  bh1750_i2c_data.sendLen = 1;
  I2cWrite(WIFI_IOT_I2C_IDX_1, (BH1750_Addr << 1) | 0x00, &bh1750_i2c_data);
  send_data[0] = 0x13; // Continuous L-Resolution Mode (测量时间 16ms)
  I2cWrite(WIFI_IOT_I2C_IDX_1, (BH1750_Addr << 1) | 0x00, &bh1750_i2c_data);
}
/***************************************************************
 * 函数名称: Start_BH1750
 * 说    明: 启动BH1750单次测量(保留兼容,连续模式下不再使用)
 ***************************************************************/
void Start_BH1750(void) {
  WifiIotI2cData bh1750_i2c_data{};
  uint8_t send_data[1] = {0x10};
  bh1750_i2c_data.sendBuf = send_data;
  bh1750_i2c_data.sendLen = 1;
  I2cWrite(WIFI_IOT_I2C_IDX_1, (BH1750_Addr << 1) | 0x00, &bh1750_i2c_data);
}
/***************************************************************
 * 函数名称: E53_SC1_Init
 * 说    明: 初始化E53_SC1
 ***************************************************************/
void E53_SC1_Init(void) {
  E53_SC1_IO_Init();
  Init_BH1750();
}
/***************************************************************
 * 函数名称: E53_SC1_Read_Data
 * 说    明: 测量光照强度(连续低分辨率模式,16ms 更新一次)
 * 返 回 值: 光照强度
 ***************************************************************/
float E53_SC1_Read_Data(void) {
  int result;
  WifiIotI2cData bh1750_i2c_data{};
  uint8_t recv_data[2] = {0};
  bh1750_i2c_data.receiveBuf = recv_data;
  bh1750_i2c_data.receiveLen = 2;
  I2cRead(WIFI_IOT_I2C_IDX_1, (BH1750_Addr << 1) | 0x01,
          &bh1750_i2c_data);                   // 读取传感器数据
  result = (recv_data[0] << 8) + recv_data[1]; // 合成数据，即光照数据
  // L-Resolution 模式灵敏度为 H-res 的 1/7.5,换算系数 = 7.5 / 1.2 = 6.25
  return (float)(result * 7);
}
/***************************************************************
 * 函数名称: Light_SetBrightness
 * 说    明: PWM 调光。percent 为感知亮度(经 γ=2.2 校正表换算成
 *           占空比);0% 切回 GPIO 拉低,100% 切回 GPIO 常亮
 *           (PWM 停止后引脚电平不定,必须切回 GPIO 输出确定电平)
 * 参    数: percent 感知亮度百分比,超界自动收敛到 0~100
 ***************************************************************/
void Light_SetBrightness(int percent) {
  percent = percent < 0 ? 0 : (percent > 100 ? 100 : percent);
  if (percent == 0) {
    if (g_pwm0_inited) {
      PwmStop(WIFI_IOT_PWM_PORT_PWM0);
    }
    IoSetFunc(WIFI_IOT_IO_NAME_GPIO_7, WIFI_IOT_IO_FUNC_GPIO_7_GPIO);
    GpioSetDir(WIFI_IOT_GPIO_IDX_7, WIFI_IOT_GPIO_DIR_OUT);
    GpioSetOutputVal(WIFI_IOT_GPIO_IDX_7, WIFI_IOT_GPIO_VALUE0);
  } else if (percent >= 100) {
    if (g_pwm0_inited) {
      PwmStop(WIFI_IOT_PWM_PORT_PWM0);
    }
    IoSetFunc(WIFI_IOT_IO_NAME_GPIO_7, WIFI_IOT_IO_FUNC_GPIO_7_GPIO);
    GpioSetDir(WIFI_IOT_GPIO_IDX_7, WIFI_IOT_GPIO_DIR_OUT);
    GpioSetOutputVal(WIFI_IOT_GPIO_IDX_7, WIFI_IOT_GPIO_VALUE1);
  } else {
    IoSetFunc(WIFI_IOT_IO_NAME_GPIO_7, WIFI_IOT_IO_FUNC_GPIO_7_PWM0_OUT);
    if (!g_pwm0_inited) {
      PwmInit(WIFI_IOT_PWM_PORT_PWM0);
      g_pwm0_inited = true;
    }
    /* 1~99% 经查表得占空比计数(最大 15650 < 16000,无溢出) */
    PwmStart(WIFI_IOT_PWM_PORT_PWM0, kGammaDuty[percent], kPwmCarrierFreq);
  }
}
/***************************************************************
 * 函数名称: Light_DutyPercent
 * 说    明: 感知亮度 % → 实际占空比 %(0~100,四舍五入)
 ***************************************************************/
int Light_DutyPercent(int percent) {
  percent = percent < 0 ? 0 : (percent > 100 ? 100 : percent);
  return (int)((kGammaDuty[percent] * 100 + kPwmCarrierFreq / 2) /
               kPwmCarrierFreq);
}
/***************************************************************
 * 函数名称: Light_StatusSet
 * 说    明: 灯状态设置(兼容旧语义,内部走 PWM 调光层)
 ***************************************************************/
void Light_StatusSet(LampStatus status) {
  Light_SetBrightness(status == LampStatus::On ? 100 : 0);
}
