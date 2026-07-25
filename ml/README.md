# ml/ — 404-snf machine learning

这是 `404-snf` 的正式机器学习工程。模型不是神经网络，而是仅有
6 个权重和 1 个偏置的 logistic-linear ridge regression；PyTorch 只负责确定性训练和张量
计算。训练同时导出便携 JSON 和 Rust/ONNX Runtime 直接加载的 `out/fatigue.onnx`。

> 这不是医疗诊断模型。没有真实受试者标签时，训练目标来自经验公式，只能用于打通数据
> 管线和冷启动，不能声称临床或安全性能。

## 文献依据与边界

检索使用 OpenAlex/Crossref 元数据并按 DOI 核对。以下论文支持的是**信号选择和实验设计**，
不是本项目具体权重：

- Martins et al. 的 60 篇研究综述指出，疲劳监测常使用运动、ECG/PPG 和呼吸信号，但多数
  研究规模小、实验室条件多，泛化能力不足
  ([Frontiers in Physiology, 2021](https://doi.org/10.3389/fphys.2021.790292))。
- Fujiwara et al. 使用 HRV 检测驾驶困倦并用 EEG 验证，说明心脏自主神经变化有信息量
  ([IEEE TBME, 2019](https://doi.org/10.1109/TBME.2018.2879346))。Burlacu et al. 的系统综述
  同时指出不同研究的灵敏度/特异度差异很大，多参数优于单参数，真实场景混杂因素明显
  ([RCM, 2021](https://doi.org/10.31083/j.rcm2203090))。
- Guede-Fernandez et al. 直接使用呼吸信号分析驾驶困倦
  ([IEEE Access, 2019](https://doi.org/10.1109/ACCESS.2019.2924481))；Nicolò et al. 的综述指出
  呼吸频率对认知负荷、体力活动和运动疲劳都敏感，因此必须与运动状态共同解释
  ([Sensors, 2020](https://doi.org/10.3390/s20216396))。
- KSS 是 1–9 级瞬时困倦自评量表，可作为采集标签之一
  ([Åkerstedt & Gillberg, 1990](<https://doi.org/10.1016/0167-8760(90)90010-B>))。

当前 TI 雷达只给聚合 HR/RR，没有逐搏间期，因此**不能计算真正的 RMSSD、SDNN、LF/HF**。
本实现只使用相对个人清醒基线的 BPM 变化。低运动量可能是专注、休息、睡眠或传感器丢失，
所以点云运动项被限制为弱证据，且不能单独触发高疲劳结论。

## 经验公式

每个 30–60 秒窗口先生成有界特征：

```text
h  = clip((HR_baseline - HR) / max(0.15*HR_baseline, 10), -2, 2)
r  = clip((RR_baseline - RR) / max(0.20*RR_baseline, 3), -2, 2)
mq = 1 - clip(rms_radial_speed / 0.10, 0, 1)
pq = 1 - clip(moving_point_fraction, 0, 1)
md = clip((long_energy - short_energy) / max(long_energy, 1e-6), -1, 1)
cr = h * r

fatigue_proxy = 100 * sigmoid(-2.20 + 0.65*h + 0.85*r
                              + 0.55*mq + 0.35*pq + 0.45*md + 0.35*cr)
```

系数是保守的工程先验，并非论文给出的通用生理定律。模型对公式生成的弱标签做 ridge 拟合，
本质是把该先验蒸馏成便携参数；有真实 `fatigue_score` 时则学习真实标签。

## 数据格式

CSV 一行对应一个窗口，必需列：

| 列                              |  单位/范围 | 来源                              |
| ------------------------------- | ---------: | --------------------------------- |
| `heart_rate_bpm`                | 30–220 bpm | `heart_rate.stabilized_bpm`       |
| `respiration_rate_bpm`          |   4–60 bpm | `respiration_rate.stabilized_bpm` |
| `rms_radial_speed_mps`          |        m/s | `activity.rms_radial_speed_mps`   |
| `moving_point_fraction`         |        0–1 | `activity.moving_point_fraction`  |
| `short_term_energy_mps2`        |     (m/s)² | `activity.short_term_energy_mps2` |
| `long_term_energy_mps2`         |     (m/s)² | `activity.long_term_energy_mps2`  |
| `baseline_heart_rate_bpm`       |        bpm | 每名受试者清醒静息校准中位数      |
| `baseline_respiration_rate_bpm` |        bpm | 同上                              |

建议额外提供 `subject_id`（用于按人切分）、`window_start` 和 `fatigue_score`。如果
`fatigue_score` 整列缺失，训练器自动使用经验公式；如果部分缺失会整体视为弱标签，避免混合
标签含义。真实采集建议同步 KSS（可映射为 `(KSS-1)/8*100`）和 3–5 分钟 PVT，并记录睡眠时长、
咖啡因、任务类型和传感器质量。训练/验证必须按受试者分组，不能把同一个人的相邻窗口随机拆开。

## 使用 uv 复现

```powershell
cd ml
uv sync

# 仅用于管线自测的合成数据
uv run fatigue-demo-data --output data/demo.csv

uv run fatigue-train --data data/demo.csv
uv run fatigue-predict --model out/fatigue-linear.json `
  --data data/demo.csv --output artifacts/predictions.csv

uv run pytest
uv run ruff check .
```

`pyproject.toml` 把 torch 固定到官方 CPU wheel 索引；`uv.lock` 固定完整依赖。训练采用闭式 ridge
解，不需要 epoch、GPU 或 DataLoader，固定输入和 seed 会产生相同 JSON/ONNX。

设备部署：`just ml-deploy root@board`。Rust 输入是六个工程特征，ONNX 输出一个
`0..100` 分数；通道有效性和十分钟基线热身不进入模型，而由 Rust 用于安全置信度。

`out/fatigue-linear.json` 保存可审计系数，`out/fatigue.onnx` 是相同参数的部署产物；两者元数据
明确标记弱标签来源，不能把当前示例训练结果作为真实人体性能结论。

## 上线前最低验证要求

1. 至少跨多个受试者和多个日期采集，不把经验公式分数当 ground truth。
2. 报告 subject-wise MAE、RMSE、等级混淆矩阵以及置信区间；单独评估运动、静息和丢点场景。
3. 与简单基线（只预测训练集均值、只用 HR、只用 RR）比较，并做特征消融。
4. 用独立人群确定报警阈值；模型输出先作为趋势提示，不直接控制安全关键执行器。
5. 若能获得逐搏间期，再新增经过伪迹清洗的 RMSSD/SDNN，而不是从聚合 BPM 反推 HRV。
