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

#ifndef __E53_SC1_H__
#define __E53_SC1_H__

#define BH1750_Addr 0x23

/***************************************************************
* 名      称: LampStatus
* 说    明：灯开关状态(仅本工程内部使用,C++ enum class)
***************************************************************/
enum class LampStatus {
    Off = 0,
    On
};

void E53_SC1_Init(void);
float E53_SC1_Read_Data(void);
void Light_StatusSet(LampStatus status);
/* PWM 调光:percent 0~100(感知亮度),0 熄灭 / 1~99 硬件 PWM(10kHz,γ=2.2) / 100 常亮 */
void Light_SetBrightness(int percent);
/* 感知亮度 % → 实际占空比 %(γ=2.2 后,0~100);自照度折算等需要物理占空比的场合用 */
int Light_DutyPercent(int percent);

#endif /* __E53_SC1_H__ */
