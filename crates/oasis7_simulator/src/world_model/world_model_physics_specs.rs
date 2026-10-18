#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PhysicsParameterSpec {
    pub key: &'static str,
    pub unit: &'static str,
    pub recommended_min: f64,
    pub recommended_max: f64,
    pub tuning_impact: &'static str,
}

const PHYSICS_PARAMETER_SPECS: [PhysicsParameterSpec; 13] = [
    PhysicsParameterSpec {
        key: "time_step_s",
        unit: "s/tick",
        recommended_min: 1.0,
        recommended_max: 60.0,
        tuning_impact: "时间步越大，单次动作跨度更大、离散误差更明显。",
    },
    PhysicsParameterSpec {
        key: "power_unit_j",
        unit: "J/power_unit",
        recommended_min: 100.0,
        recommended_max: 10_000.0,
        tuning_impact: "影响电力单位到焦耳的映射，与移动/发电口径联动。",
    },
    PhysicsParameterSpec {
        key: "max_move_distance_cm_per_tick",
        unit: "cm/tick",
        recommended_min: 100.0,
        recommended_max: 5_000_000.0,
        tuning_impact: "限制单 tick 最大位移，防止瞬移跨域。",
    },
    PhysicsParameterSpec {
        key: "max_move_speed_cm_per_s",
        unit: "cm/s",
        recommended_min: 100.0,
        recommended_max: 500_000.0,
        tuning_impact: "限制速度上限，约束动力学可解释区间。",
    },
    PhysicsParameterSpec {
        key: "radiation_floor",
        unit: "power_unit/tick",
        recommended_min: 0.0,
        recommended_max: 10.0,
        tuning_impact: "外部背景辐射通量基线，抬升全局可采下限。",
    },
    PhysicsParameterSpec {
        key: "radiation_floor_cap_per_tick",
        unit: "power_unit/tick",
        recommended_min: 0.0,
        recommended_max: 50.0,
        tuning_impact: "背景通量采集上限，限制 floor 造能强度。",
    },
    PhysicsParameterSpec {
        key: "radiation_decay_k",
        unit: "cm^-1",
        recommended_min: 1e-7,
        recommended_max: 1e-4,
        tuning_impact: "介质吸收强度，数值越大近距离优势越明显。",
    },
    PhysicsParameterSpec {
        key: "max_harvest_per_tick",
        unit: "power_unit/tick",
        recommended_min: 1.0,
        recommended_max: 500.0,
        tuning_impact: "限制单 tick 采集峰值，影响高辐射区收益上限。",
    },
    PhysicsParameterSpec {
        key: "thermal_capacity",
        unit: "heat_unit",
        recommended_min: 10.0,
        recommended_max: 1_000.0,
        tuning_impact: "热惯性参数，越大越不容易过热。",
    },
    PhysicsParameterSpec {
        key: "thermal_dissipation",
        unit: "heat_unit/tick",
        recommended_min: 1.0,
        recommended_max: 50.0,
        tuning_impact: "散热基准强度，提升后稳态温度下降。",
    },
    PhysicsParameterSpec {
        key: "thermal_dissipation_gradient_bps",
        unit: "bps",
        recommended_min: 1_000.0,
        recommended_max: 50_000.0,
        tuning_impact: "温差梯度放大系数，控制高热状态散热斜率。",
    },
    PhysicsParameterSpec {
        key: "heat_factor",
        unit: "heat_unit/power_unit",
        recommended_min: 1.0,
        recommended_max: 20.0,
        tuning_impact: "采集转热系数，越高越容易触发热降效。",
    },
    PhysicsParameterSpec {
        key: "erosion_rate",
        unit: "tick^-1 (scaled)",
        recommended_min: 1e-7,
        recommended_max: 1e-4,
        tuning_impact: "磨损基准速率，影响长期维护与硬件损耗。",
    },
];

pub fn physics_parameter_specs() -> &'static [PhysicsParameterSpec] {
    &PHYSICS_PARAMETER_SPECS
}
