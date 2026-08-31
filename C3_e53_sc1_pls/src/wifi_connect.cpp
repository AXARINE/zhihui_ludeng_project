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

#include <stdio.h>
#include <string.h>
#include <unistd.h>

#include "sdk_cxx.h"
#include "wifi_connect.h"

#define DEF_TIMEOUT 15
#define ONE_SECOND 1

#define SELECT_WIFI_SECURITYTYPE WIFI_SEC_TYPE_PSK

#define SELECT_WLAN_PORT "wlan0"

/* 扫描结果缓冲:.bss 静态分配,一次 3.5KB。
 * C 版这里每次 WifiConnect 都 malloc 且从不 free——开机重试循环
 * 约 20s 一轮、每轮漏 3.5KB,路由器长时间不在场会把 352KB SRAM
 * 耗光。静态化后泄露在类型层面不存在。 */
static WifiScanInfo g_scan_info[WIFI_SCAN_HOTSPOT_LIMIT];

static int g_staScanSuccess = 0;
static int g_ConnectSuccess = 0;
static int ssid_count = 0;
static WifiEvent g_wifiEventHandler{};
static WifiErrorCode error;

static void WiFiInit(void);
static void WaitSacnResult(void);
static int WaitConnectResult(void);

/* 交给 C SDK 事件表的回调:C 语言链接,与函数字段类型严格一致 */
extern "C" {

static void OnWifiScanStateChangedHandler(int state, int size) {
  if (size > 0) {
    ssid_count = size;
    g_staScanSuccess = 1;
  }
  printf("callback function for wifi scan:%d, %d\r\n", state, size);
}

static void OnWifiConnectionChangedHandler(int state, WifiLinkedInfo *info) {
  if (info == NULL) {
    printf("WifiConnectionChanged:info is null, stat is %d.\n", state);
  } else {
    if (state == WIFI_STATE_AVALIABLE) {
      g_ConnectSuccess = 1;
    } else {
      g_ConnectSuccess = 0;
    }
  }
}

static void OnHotspotStaJoinHandler(StationInfo *info) {
  (void)info;
  printf("STA join AP\n");
}

static void OnHotspotStaLeaveHandler(StationInfo *info) {
  (void)info;
  printf("HotspotStaLeave:info is null.\n");
}

static void OnHotspotStateChangedHandler(int state) {
  printf("HotspotStateChanged:state is %d.\n", state);
}

} // extern "C"

extern "C" int WifiConnect(const char *ssid, const char *psk) {
  unsigned int size = WIFI_SCAN_HOTSPOT_LIMIT;
  static struct netif *g_lwip_netif = NULL;

  osDelay(200);
  printf("<--System Init-->\r\n");

  //初始化WIFI
  WiFiInit();

  //使能WIFI
  if (EnableWifi() != WIFI_SUCCESS) {
    printf("EnableWifi failed, error = %d\r\n", error);
    return -1;
  }

  //判断WIFI是否激活
  if (IsWifiActive() == 0) {
    printf("Wifi station is not actived.\r\n");
    return -1;
  }

  //轮询查找WiFi列表(扫描结果写入静态缓冲 g_scan_info)
  do {
    //重置标志位
    ssid_count = 0;
    g_staScanSuccess = 0;

    //开始扫描
    Scan();

    //等待扫描结果
    WaitSacnResult();

    //获取扫描列表
    error = GetScanInfoList(g_scan_info, &size);

  } while (g_staScanSuccess != 1);
  //打印WiFi列表
  printf("********************\r\n");
  for (uint8_t i = 0; i < ssid_count; i++) {
    printf("no:%03d, ssid:%-30s, rssi:%5d\r\n", i + 1, g_scan_info[i].ssid,
           g_scan_info[i].rssi / 100);
  }
  printf("********************\r\n");

  //连接指定的WiFi热点
  for (uint8_t i = 0; i < ssid_count; i++) {
    if (strcmp(ssid, g_scan_info[i].ssid) == 0) {
      int result;

      printf("Select:%3d wireless, Waiting...\r\n", i + 1);

      //拷贝要连接的热点信息
      WifiDeviceConfig select_ap_config{};
      strncpy(select_ap_config.ssid, g_scan_info[i].ssid,
              sizeof(select_ap_config.ssid) - 1);
      strncpy(select_ap_config.preSharedKey, psk,
              sizeof(select_ap_config.preSharedKey) - 1);
      select_ap_config.securityType = SELECT_WIFI_SECURITYTYPE;

      if (AddDeviceConfig(&select_ap_config, &result) == WIFI_SUCCESS) {
        if (ConnectTo(result) == WIFI_SUCCESS && WaitConnectResult() == 1) {
          printf("WiFi connect succeed!\r\n");
          g_lwip_netif = netifapi_netif_find(SELECT_WLAN_PORT);
          break;
        }
      }
    }

    if (i == ssid_count - 1) {
      printf("ERROR: No wifi as expected\r\n");
      return -1; // 交还调用方重试,不再原地死等
    }
  }
  //启动DHCP
  if (g_lwip_netif) {
    dhcp_start(g_lwip_netif);
    printf("begain to dhcp\r\n");
  }

  //等待DHCP
  for (;;) {
    if (dhcp_is_bound(g_lwip_netif) == ERR_OK) {
      printf("<-- DHCP state:OK -->\r\n");

      //打印获取到的IP信息
      netifapi_netif_common(g_lwip_netif, dhcp_clients_info_show, NULL);
      break;
    }

    printf("<-- DHCP state:Inprogress -->\r\n");
    osDelay(100);
  }

  osDelay(100);

  return 0;
}

extern "C" int WifiConnectStatus(void) { return g_ConnectSuccess; }

static void WiFiInit(void) {
  printf("<--Wifi Init-->\r\n");
  g_wifiEventHandler.OnWifiScanStateChanged = OnWifiScanStateChangedHandler;
  g_wifiEventHandler.OnWifiConnectionChanged = OnWifiConnectionChangedHandler;
  g_wifiEventHandler.OnHotspotStaJoin = OnHotspotStaJoinHandler;
  g_wifiEventHandler.OnHotspotStaLeave = OnHotspotStaLeaveHandler;
  g_wifiEventHandler.OnHotspotStateChanged = OnHotspotStateChangedHandler;
  error = RegisterWifiEvent(&g_wifiEventHandler);
  if (error != WIFI_SUCCESS) {
    printf("register wifi event fail!\r\n");
  } else {
    printf("register wifi event succeed!\r\n");
  }
}

static void WaitSacnResult(void) {
  int scanTimeout = DEF_TIMEOUT;
  while (scanTimeout > 0) {
    sleep(ONE_SECOND);
    scanTimeout--;
    if (g_staScanSuccess == 1) {
      printf("WaitSacnResult:wait success[%d]s\n", (DEF_TIMEOUT - scanTimeout));
      break;
    }
  }
  if (scanTimeout <= 0) {
    printf("WaitSacnResult:timeout!\n");
  }
}

static int WaitConnectResult(void) {
  int ConnectTimeout = DEF_TIMEOUT;
  while (ConnectTimeout > 0) {
    sleep(ONE_SECOND);
    ConnectTimeout--;
    if (g_ConnectSuccess == 1) {
      printf("WaitConnectResult:wait success[%d]s\n",
             (DEF_TIMEOUT - ConnectTimeout));
      break;
    }
  }
  if (ConnectTimeout <= 0) {
    printf("WaitConnectResult:timeout!\n");
    return 0;
  }

  return 1;
}
