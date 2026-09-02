/*
 * 应用私有配置模板。复制为 app_config.h 并填入真实凭据。
 * app_config.h 已被 .gitignore 忽略,不会进 git。
 */

#ifndef __APP_CONFIG_H__
#define __APP_CONFIG_H__

#define CONFIG_WIFI_SSID "你的2.4G Wi-Fi 名称"
#define CONFIG_WIFI_PWD "你的Wi-Fi密码"

#define CONFIG_APP_DEVICEID "IoTDA 注册设备后生成的设备ID"
#define CONFIG_APP_DEVICEPWD "设备密钥"

/* IoTDA 实例设备侧域名(MQTT 接入地址,非保密项但换实例需改,
 * 形如 xxx.st1.iotda-device.{region}.myhuaweicloud.com,
 * 在控制台 → 实例 → 接入信息 查看;不要填应用侧域名/区域共享域名) */
#define CONFIG_APP_SERVERIP "xxx.st1.iotda-device.cn-south-1.myhuaweicloud.com"

#endif /* __APP_CONFIG_H__ */
