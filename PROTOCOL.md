# 404-SNF BLE Telemetry Protocol v1

本文件定义 CA35 通过 BLE 向手机或 Web Bluetooth 客户端发布毫米波雷达遥测数据的协议。
协议优先保证心率、呼吸率和设备状态的及时性；人体姿态和点云属于可选高带宽流。

> 状态：设计完成，尚未实现。当前 `snf-ble` 仍是 BlueZ GATT 脚手架，
> `apps/web/app/utils/protocol.ts` 中的 `5f04/5f05` UUID 和 9 字节疲劳包是
> legacy placeholder，不属于本协议 v1。

## 1. 目标与非目标

目标：

- 以低延迟持续发布心率、呼吸率和运动质量状态。
- 支持不同关节点模型的人体姿态输出。
- 支持按需开启、降采样和分片传输 3D 点云。
- 在任意 ATT MTU 下工作，并能检测逻辑消息丢失。
- Rust、原生移动端和 Web Bluetooth 使用相同的小端字节布局。
- 生命体征不得因姿态或点云拥塞而排队延迟。

非目标：

- 不通过 BLE 传输原始 ADC、range FFT、热图或完整雷达 cube。
- 不从 15 点显示波形重新计算 HRV。当前 TI TLV 只提供聚合心率，不能支持可靠 HRV。
- 不定义姿态估计算法；协议仅承载未来模型的结果。
- 不保证 BLE 点云等价于雷达原始输出。设备可以基于 ROI、SNR 和带宽进行降采样。

## 2. 当前数据能力

| 数据       | 当前雷达代码                                          | BLE v1                                        |
| ---------- | ----------------------------------------------------- | --------------------------------------------- |
| 心率 BPM   | 需要 `vital-signs` feature 和 TI Vital Signs firmware | Vitals characteristic                         |
| 呼吸率 BPM | 需要 `vital-signs` feature 和 TI Vital Signs firmware | Vitals characteristic                         |
| 粗粒度运动 | 已由点云径向速度计算                                  | Vitals characteristic 中的活动字段            |
| 3D 点云    | Out-of-Box 和 Vital Signs parser 均已支持             | Point Cloud characteristic                    |
| 人体姿态   | 尚未实现，需要时序点云融合和姿态模型                  | Pose characteristic，未实现时报告 unavailable |
| 疲劳评分   | 当前为固定值 stub                                     | Fatigue characteristic，可选                  |

客户端必须根据 Protocol Info 的 capability bits 和每条消息的状态位决定是否显示数据，
不能把缺失值显示为 `0 bpm` 或有效姿态。

## 3. GATT 结构

v1 使用一组真正的私有 128-bit UUID。当前占位 UUID 尚未实际部署，因此 v1 不保留其兼容性。

| 名称                  | UUID                                   | 属性                          | 用途                       |
| --------------------- | -------------------------------------- | ----------------------------- | -------------------------- |
| SNF Telemetry Service | `7b9f0001-6b44-4d2a-9f36-4040534e4600` | Primary service               | 所有 SNF 遥测              |
| Protocol Info         | `7b9f0001-6b44-4d2a-9f36-4040534e4601` | Read                          | 版本、能力和限制           |
| Stream Control        | `7b9f0001-6b44-4d2a-9f36-4040534e4602` | Write with response, Indicate | 开关流、设置速率、命令响应 |
| Device Status         | `7b9f0001-6b44-4d2a-9f36-4040534e4603` | Read, Notify                  | 运行时间、丢帧和错误       |
| Vitals                | `7b9f0001-6b44-4d2a-9f36-4040534e4604` | Read, Notify                  | 心率、呼吸率和运动质量     |
| Fatigue               | `7b9f0001-6b44-4d2a-9f36-4040534e4605` | Read, Notify                  | 可选疲劳模型输出           |
| Pose                  | `7b9f0001-6b44-4d2a-9f36-4040534e4606` | Notify                        | 跟踪人体的 3D 关节点       |
| Point Cloud           | `7b9f0001-6b44-4d2a-9f36-4040534e4607` | Notify                        | 降采样 3D 点云             |

建议广播：

- Local Name：`404-SNF`
- Complete List of 128-bit Service UUIDs：SNF Telemetry Service
- 不在广播包中放生命体征或人员数据。

标准 Device Information Service 和 Battery Service 可以另外注册，但不替代上表中的状态消息。

## 4. 字节序与数值约定

- 所有多字节整数均为 little-endian。
- 不在 wire format 中发送 Rust enum、C struct 内存或未定义 padding。
- 速率使用 `u16`，单位为 `0.01 bpm`。`0xffff` 表示 unavailable。
- 置信度使用 `u8`，`0..=100` 表示 `0%..=100%`。
- 坐标使用 `i16` 毫米：`x` 向右、`y` 从雷达向外、`z` 向上。
- 径向速度正值表示远离雷达。
- 序号和时间戳回绕按无符号整数模运算处理。
- 保留字段发送端必须写零，接收端必须忽略。

## 5. Protocol Info

Protocol Info 是固定 24 字节 Read characteristic，不使用遥测消息头。

| Offset | Type    | 字段                 | 说明                           |
| -----: | ------- | -------------------- | ------------------------------ |
|      0 | `u8[4]` | magic                | ASCII `SNF1`                   |
|      4 | `u8`    | major                | `1`                            |
|      5 | `u8`    | minor                | 初始为 `0`                     |
|      6 | `u8`    | telemetry_header_len | `16`                           |
|      7 | `u8`    | coordinate_frame     | `1` = x-right/y-out/z-up       |
|      8 | `u32`   | capabilities         | 见下表                         |
|     12 | `u16`   | max_point_count      | 单个逻辑点云帧的上限           |
|     14 | `u8`    | max_pose_joints      | 单个人体的最大关节点数         |
|     15 | `u8`    | max_subjects         | 当前建议为 `1`                 |
|     16 | `u32`   | boot_id              | 每次启动随机变化，用于识别重启 |
|     20 | `u32`   | build_id             | 固件构建 ID；`0` 表示未知      |

Capability bits：

| Bit | 名称                  |
| --: | --------------------- |
|   0 | `VITALS`              |
|   1 | `FATIGUE`             |
|   2 | `POSE_3D`             |
|   3 | `POINT_CLOUD_3D`      |
|   4 | `MULTI_SUBJECT`       |
|   5 | `BATTERY_STATUS`      |
|   6 | `ENCRYPTION_REQUIRED` |

## 6. 统一遥测消息头

除 Protocol Info 和客户端写入的 Control Request 外，所有 characteristic value 都以同一个
16 字节头开始。一个逻辑消息可以拆成多条 notification；每个分片都重复此头。

| Offset | Type  | 字段              | 说明                                    |
| -----: | ----- | ----------------- | --------------------------------------- |
|      0 | `u8`  | protocol_major    | `1`                                     |
|      1 | `u8`  | message_type      | 见消息类型表                            |
|      2 | `u8`  | flags             | 见 flags 表                             |
|      3 | `u8`  | header_len        | `16`                                    |
|      4 | `u32` | sequence          | 每种 message type 独立递增              |
|      8 | `u32` | timestamp_ms      | 设备启动后的 monotonic 毫秒数           |
|     12 | `u16` | total_payload_len | 完整逻辑 payload 长度，不含头           |
|     14 | `u16` | fragment_offset   | 当前分片数据在逻辑 payload 中的字节偏移 |

Message types：

|  Value | 消息             |
| -----: | ---------------- |
| `0x10` | Device Status    |
| `0x20` | Vitals           |
| `0x21` | Fatigue          |
| `0x30` | Pose             |
| `0x31` | Point Cloud      |
| `0x40` | Control Response |

Flags：

| Bit | 名称             | 说明                                 |
| --: | ---------------- | ------------------------------------ |
|   0 | `MORE_FRAGMENTS` | 当前分片之后还有数据                 |
|   1 | `SNAPSHOT`       | 响应一次性 snapshot 请求             |
|   2 | `DEGRADED`       | 数据可显示，但质量下降               |
|   3 | `STALE`          | 数据沿用上一有效值，不是当前帧新结果 |

分片规则：

1. notification 总长度不得超过 `ATT_MTU - 3`。
2. 客户端按 `(message_type, sequence)` 建立重组缓冲区，并按 `fragment_offset` 写入。
3. `MORE_FRAGMENTS=0` 且已覆盖 `0..total_payload_len` 时消息完成。
4. 新序号到达、超时 500 ms、offset 越界或内容重叠冲突时，丢弃未完成消息。
5. 不重传过期实时帧。sequence 跳变只表示丢帧，不表示设备错误。
6. 在默认 ATT MTU 23 下每个分片只有 4 字节 payload，效率很低；客户端应请求较大 MTU，
   但协议不得依赖 Web Bluetooth 能读取或设置 MTU。

## 7. Vitals payload (`0x20`)

固定 24 字节，建议默认 2 Hz，允许 1..10 Hz。

| Offset | Type  | 字段                     | 说明                            |
| -----: | ----- | ------------------------ | ------------------------------- |
|      0 | `u16` | subject_id               | TI tracking ID；未知为 `0xffff` |
|      2 | `u16` | status_flags             | 见下表                          |
|      4 | `u16` | heart_rate_x100          | BPM x 100；无效为 `0xffff`      |
|      6 | `u16` | respiration_rate_x100    | BPM x 100；无效为 `0xffff`      |
|      8 | `u8`  | heart_confidence         | `0..100`                        |
|      9 | `u8`  | respiration_confidence   | `0..100`                        |
|     10 | `u8`  | activity_confidence      | 点数支持度，不是医疗置信度      |
|     11 | `u8`  | reserved                 | `0`                             |
|     12 | `u32` | motion_energy_um2_s2     | 平均径向速度平方乘 `1_000_000`  |
|     16 | `u16` | rms_speed_mm_s           | RMS 径向速度，mm/s              |
|     18 | `u16` | moving_fraction_q15      | 运动点占比，`0..32767`          |
|     20 | `u16` | range_bin                | TI vital result 的 range bin    |
|     22 | `i16` | breathing_deviation_q8_8 | vendor unit x 256               |

Status flags：

| Bit | 名称                   |
| --: | ---------------------- |
|   0 | `SUBJECT_TRACKED`      |
|   1 | `HEART_VALID`          |
|   2 | `RESPIRATION_VALID`    |
|   3 | `WARMING_UP`           |
|   4 | `MOTION_CONTAMINATED`  |
|   5 | `VENDOR_VALUE_INVALID` |
|   6 | `RADAR_GAP`            |

状态优先于数值。例如 `MOTION_CONTAMINATED` 时可以发送最近稳定 BPM 并设置遥测头的
`STALE`；UI 必须明确显示质量告警，不能把该值当作新的可靠测量。

## 8. Fatigue payload (`0x21`)

固定 12 字节，只有 capability `FATIGUE` 存在时才发布。

| Offset | Type  | 字段           | 说明                                                 |
| -----: | ----- | -------------- | ---------------------------------------------------- |
|      0 | `u8`  | level          | `0..100`                                             |
|      1 | `u8`  | confidence     | `0..100`                                             |
|      2 | `u16` | status_flags   | bit 0 valid，bit 1 warming，bit 2 insufficient input |
|      4 | `u32` | model_revision | 模型版本或哈希缩写                                   |
|      8 | `u32` | reserved       | `0`                                                  |

## 9. Pose payload (`0x30`)

建议默认 10 Hz，允许 1..20 Hz。当前代码没有姿态模型，因此设备必须清除 `POSE_3D`
capability，而不是发送全零骨架。

Pose header：

| Offset | Type  | 字段             | 说明                                         |
| -----: | ----- | ---------------- | -------------------------------------------- |
|      0 | `u16` | subject_id       | 跟踪 ID                                      |
|      2 | `u8`  | model_id         | `1` COCO-17，`2` BlazePose-33                |
|      3 | `u8`  | joint_count      | 后续 joint 数量                              |
|      4 | `u8`  | coordinate_frame | 必须与 Protocol Info 一致                    |
|      5 | `u8`  | pose_flags       | bit 0 tracked，bit 1 inferred，bit 2 partial |
|      6 | `u16` | reserved         | `0`                                          |

每个 joint 固定 8 字节：

| Offset | Type  | 字段                  |
| -----: | ----- | --------------------- |
|      0 | `u8`  | joint_id              |
|      1 | `u8`  | confidence (`0..100`) |
|      2 | `i16` | x_mm                  |
|      4 | `i16` | y_mm                  |
|      6 | `i16` | z_mm                  |

完整 payload 长度必须等于 `8 + joint_count * 8`。关节点语义由 `model_id` 固定，不能在
同一个 model ID 下改变编号。需要新增模型时分配新的 ID。

## 10. Point Cloud payload (`0x31`)

点云默认关闭。建议用户进入 3D 页面后才请求 5 Hz、最多 96 点；允许范围为 1..10 Hz。

Point Cloud header：

| Offset | Type  | 字段             | 说明                  |
| -----: | ----- | ---------------- | --------------------- |
|      0 | `u16` | subject_id       | 未关联人员为 `0xffff` |
|      2 | `u16` | point_count      | 后续点数量            |
|      4 | `u8`  | point_format     | v1 只定义 `1`         |
|      5 | `u8`  | coordinate_frame | x-right/y-out/z-up    |
|      6 | `u16` | reserved         | `0`                   |

Point format 1 每点固定 8 字节：

| Offset | Type  | 字段                  | 说明                               |
| -----: | ----- | --------------------- | ---------------------------------- |
|      0 | `i16` | x_mm                  | 超范围点应在发送前丢弃，不饱和截断 |
|      2 | `i16` | y_mm                  | 同上                               |
|      4 | `i16` | z_mm                  | 同上                               |
|      6 | `i8`  | radial_velocity_2cm_s | 数值乘 0.02 m/s                    |
|      7 | `u8`  | snr_half_db           | 数值乘 0.5 dB；未知为 `0xff`       |

完整 payload 长度必须等于 `8 + point_count * 8`。发送端按以下顺序降采样：

1. 移除非有限、ROI 外和低 SNR 点。
2. 保留人体附近或姿态模型使用的点。
3. 对剩余点做空间 voxel/均匀抽样，直到 `max_points`。
4. 不得只截取 parser 返回数组的前 N 个点，因为这会产生空间偏差。

## 11. Device Status payload (`0x10`)

固定 20 字节，连接后立即 Read，之后建议 1 Hz Notify。

| Offset | Type  | 字段                  | 说明                           |
| -----: | ----- | --------------------- | ------------------------------ |
|      0 | `u32` | uptime_s              | 设备运行时间                   |
|      4 | `u16` | active_streams        | 与 Control 的 stream mask 相同 |
|      6 | `u16` | last_error            | `0` 表示无错误                 |
|      8 | `u16` | dropped_pose_frames   | 饱和计数                       |
|     10 | `u16` | dropped_point_frames  | 饱和计数                       |
|     12 | `u16` | radar_gap_count       | 饱和计数                       |
|     14 | `u16` | battery_mv            | 未提供为 `0xffff`              |
|     16 | `i16` | processor_temp_x100_c | 未提供为 `0x7fff`              |
|     18 | `u16` | reserved              | `0`                            |

## 12. Stream Control

客户端使用 Write with response。请求头固定 8 字节，后接 opcode payload：

| Offset | Type  | 字段                 |
| -----: | ----- | -------------------- |
|      0 | `u8`  | protocol_major (`1`) |
|      1 | `u8`  | opcode               |
|      2 | `u16` | request_id           |
|      4 | `u16` | payload_len          |
|      6 | `u16` | reserved (`0`)       |

Opcodes：

|  Value | 名称               | Payload                             |
| -----: | ------------------ | ----------------------------------- |
| `0x01` | `SET_STREAMS`      | 8 字节 stream settings              |
| `0x02` | `SET_SUBJECT`      | `u16 subject_id`，`0xffff` 自动选择 |
| `0x03` | `REQUEST_SNAPSHOT` | `u16 stream_mask`                   |
| `0x04` | `PING`             | 最多 16 字节原样返回                |

`SET_STREAMS` payload：

| Offset | Type  | 字段           |
| -----: | ----- | -------------- |
|      0 | `u16` | stream_mask    |
|      2 | `u8`  | vitals_hz      |
|      3 | `u8`  | pose_hz        |
|      4 | `u8`  | point_cloud_hz |
|      5 | `u8`  | max_points     |
|      6 | `u16` | reserved       |

Stream mask bits：bit 0 status、bit 1 vitals、bit 2 fatigue、bit 3 pose、bit 4 point cloud。
设备可以降低客户端请求的速率，但不得静默提高。实际值通过 Control Response 返回。

Control Response (`0x40`) payload：

| Offset | Type  | 字段                                                                     |
| -----: | ----- | ------------------------------------------------------------------------ |
|      0 | `u16` | request_id                                                               |
|      2 | `u8`  | opcode                                                                   |
|      3 | `u8`  | result (`0` success，`1` unsupported，`2` invalid，`3` busy，`4` denied) |
|      4 | `u16` | effective_stream_mask                                                    |
|      6 | `u8`  | effective_vitals_hz                                                      |
|      7 | `u8`  | effective_pose_hz                                                        |
|      8 | `u8`  | effective_point_cloud_hz                                                 |
|      9 | `u8`  | effective_max_points                                                     |

使用 Indicate 发送响应，保证命令结果得到 ATT 层确认。

## 13. 默认配置与带宽预算

连接后默认：status 1 Hz、vitals 2 Hz、fatigue 2 Hz、pose off、point cloud off。

推荐可视化配置（单人）：

| 流             |  速率 | 典型逻辑大小 | 纯 payload 量级 |
| -------------- | ----: | -----------: | --------------: |
| Status         |  1 Hz |         20 B |     0.16 kbit/s |
| Vitals         |  2 Hz |         24 B |     0.38 kbit/s |
| Fatigue        |  2 Hz |         12 B |     0.19 kbit/s |
| COCO-17 Pose   | 10 Hz |        144 B |     11.5 kbit/s |
| 96-point Cloud |  5 Hz |        776 B |     31.0 kbit/s |

实际空口带宽还包括 16 字节分片头、ATT/L2CAP/链路开销和连接参数。弱连接时优先级必须为：

1. Control response
2. Vitals
3. Device status
4. Fatigue
5. Pose
6. Point cloud

背压时丢弃整个旧 pose/point-cloud 逻辑帧，绝不能让它们在队列中累积。Vitals 队列只保留最新值。

## 14. 连接、恢复与 UI 行为

- 连接后依次 Read Protocol Info、订阅 Status/Vitals，再按页面需要开启 Pose/Point Cloud。
- `(boot_id, message_type, sequence)` 共同标识消息序列；boot ID 变化后清空所有历史滤波状态。
- 断线后采用有限指数退避重连，不缓存并回放旧实时数据。
- UI 必须同时显示数值、更新时间和质量状态。
- 超过 2 秒没有 Vitals 时将数值标记 stale；超过 5 秒隐藏实时数值并显示 disconnected。
- 点云或姿态页面离开前发送 `SET_STREAMS` 关闭对应流。

手机端支持边界：

- Android Chrome/Edge 可以使用 Web Bluetooth 连接当前 Nuxt PWA。
- iOS Safari 不提供 Web Bluetooth。iPhone 需要 CoreBluetooth 原生应用，或使用 Capacitor 等
  容器并通过原生 BLE plugin 将相同二进制协议交给 Web UI。
- 浏览器和原生客户端必须共享 golden vectors，不能分别解释字节布局。
- 如果链路只能提供接近默认 MTU 23 的有效负载，应自动关闭点云并降低姿态速率，
  但继续维持 Status 和 Vitals。

## 15. 安全与隐私

心率、呼吸、姿态和点云均属于敏感人体数据。生产模式要求：

- 使用 LE Secure Connections 配对和加密，拒绝未加密连接订阅遥测或写 Control。
- 首次配对需要设备上的物理确认或一次性配对窗口，不能永久 Just Works 可配对。
- 默认不持久化点云和姿态；日志只记录计数、错误和带宽，不记录人体帧。
- Web Bluetooth 页面必须运行在安全上下文，并明确告知用户正在连接哪个设备。
- 广播包不得包含生命体征、人员 ID 或疲劳值。

开发模式可以通过显式构建配置关闭配对要求，但设备状态必须报告该模式，且不得用于真实用户数据。

## 16. 版本兼容

- `major` 不兼容时客户端必须停止解析，并只显示升级提示。
- `minor` 只允许增加 capability、flags、opcode 或尾部字段。
- 接收端忽略未知 flag、未知 message type 和已知 payload 尾部的新增字段。
- 发送端不得改变 v1 已定义 offset、缩放或枚举含义。
- 新 point format 或 pose model 必须分配新 ID。

## 17. 实施顺序

1. 在 `snf-ble` 中实现 Protocol Info、Control、Status 和 Vitals，不启用点云。
2. 在 CA35 应用中真正连接 `RadarStream -> IndicatorEngine -> BleTransport`。
3. 在 Web 客户端实现统一头、分片重组、sequence/boot ID 和质量状态。
4. 增加 Point Cloud，先以 5 Hz / 96 点验证 Android、iOS 原生客户端和 Web Bluetooth。
5. 完成人体姿态模型后再声明 `POSE_3D` capability 并开启 Pose characteristic。
6. 为每种 payload 增加 Rust 编码 golden vectors，并用 TypeScript 对同一向量解码测试。
