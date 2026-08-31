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

/*
 * SDK 头集中包含(C++ 迁移专用,仅供 .cpp 文件包含)。
 * ohos_init.h / cmsis_os2.h / cJSON.h 自带 extern "C" 守卫,直接包含;
 * wifiiot / wifi_lite / lwip_sack / iot_link 的头无守卫,统一在
 * extern "C" 块内包含(已逐一核实其中无 C++ 关键字标识符;
 * 嵌套 extern "C" 合法,自带守卫的头被包进块内也无副作用)。
 */

#ifndef __SDK_CXX_H__
#define __SDK_CXX_H__

#include "ohos_init.h"
#include "cmsis_os2.h"
#include <cJSON.h>

extern "C" {
#include "hos_types.h"
#include "wifiiot_errno.h"
#include "wifiiot_gpio.h"
#include "wifiiot_gpio_ex.h"
#include "wifiiot_i2c.h"
#include "wifiiot_i2c_ex.h"
#include "wifiiot_pwm.h"

#include "wifi_device.h"
#include "lwip/netif.h"
#include "lwip/netifapi.h"
#include "lwip/ip4_addr.h"
#include "lwip/api_shell.h"

#include <queue.h>
#include <oc_mqtt_al.h>
#include <oc_mqtt_profile.h>
#include <dtls_al.h>
#include <mqtt_al.h>
}

#endif /* __SDK_CXX_H__ */
