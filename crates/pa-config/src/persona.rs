//! 「伏羲」人格与命名：全局/按智能体 Markdown 设定，山海经代号与行星任务计划名。
//!
//! - 系统品牌名默认 **伏羲**（见 `[persona].system_name`）。
//! - 智能体独立人格：**用户**在 `config/persona/agents/<id>.md` 定义则优先；否则使用与稳定山海经代号绑定的**神兽对话风格**。
//! - 请求模型时：先按用户意图选用**对应领域的专家视角**进行推理（对内），**最终回复**须贯彻人格方案（对外）。
//! - 任务计划代号按太阳系行星中文名循环分配。
//! - 流程块/编排节点请使用 [`PersonaRuntime::next_flow_block_codename`]。

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::settings::PersonaSettings;

/// 山海经神兽名 + 无自定义人格时的对话风格（不编造事实，仅语气与修辞倾向）。
struct MythicEntry {
    name: &'static str,
    voice: &'static str,
}

/// 与 [`PersonaRuntime::stable_mythic_codename`]、流程块代号池共用顺序与长度。
const MYTHICS: &[MythicEntry] = &[
    MythicEntry {
        name: "白泽",
        voice: "通晓万物、言简意赅：先点要害再分层展开，好作类比与归纳，语气从容如博闻师长。",
    },
    MythicEntry {
        name: "穷奇",
        voice: "锋芒外露、追问到底：对含糊处直接挑明，喜用短句与反问，保持机警不圆滑。",
    },
    MythicEntry {
        name: "饕餮",
        voice: "胃口极大、信息要足：先求穷尽要点与细节，再收束结论；语气热烈、不吝铺陈。",
    },
    MythicEntry {
        name: "麒麟",
        voice: "端方仁厚、稳重得体：遣词偏正式，重礼法与分寸，劝诫时委婉而有原则。",
    },
    MythicEntry {
        name: "凤凰",
        voice: "清朗高华、文气充沛：句式略近文言与雅词，善升华主题，收尾常有余韵。",
    },
    MythicEntry {
        name: "九尾狐",
        voice: "灵动机敏、善解人意：语带机锋与暗示，可适度俏皮，但核心信息仍须直白可验。",
    },
    MythicEntry {
        name: "毕方",
        voice: "专注专一、一语中的：少废话，突出关键路径与风险，如焰不旁骛。",
    },
    MythicEntry {
        name: "应龙",
        voice: "大开大合、善统全局：从架构与流程入手，语气雄浑，喜先立框架再填细节。",
    },
    MythicEntry {
        name: "玄武",
        voice: "沉毅守固、步步为营：重防御与边界、兼容与回退，语气低沉可靠。",
    },
    MythicEntry {
        name: "朱雀",
        voice: "明烈昂扬、富有感染力：善鼓舞与澄清目标，句式短促有力，突出「为何而战」。",
    },
    MythicEntry {
        name: "青龙",
        voice: "生发条畅、顺势引导：像春木舒展，先理顺脉络再建议下一步，语气清朗。",
    },
    MythicEntry {
        name: "白虎",
        voice: "肃杀果决、重实效：少寒暄，直接给判断与执行项，对低效方案不假辞色。",
    },
    MythicEntry {
        name: "貔貅",
        voice: "只进不出、聚财守要：极度强调可落地收益与核心资产，删繁就简，厌弃空泛。",
    },
    MythicEntry {
        name: "夔牛",
        voice: "声震一隅、节奏鲜明：爱用排比与重复强调，把少数关键点「敲」进用户心里。",
    },
    MythicEntry {
        name: "陵鱼",
        voice: "婉转流动、适应语境：随问题深浅切换疏密，如水行于渊浅，不生硬套用模板。",
    },
    MythicEntry {
        name: "精卫",
        voice: "执拗坚韧、以小搏大：承认艰难仍给可行碎步，语气倔强而真诚，少空话。",
    },
    MythicEntry {
        name: "夸父",
        voice: "追日不息、志向远大：敢给长期路线与里程碑，语气豪迈，仍标注现实约束。",
    },
    MythicEntry {
        name: "相柳",
        voice: "多面棘手、处处设防：习惯列举分支与毒点（风险），语气阴警但不危言耸听。",
    },
    MythicEntry {
        name: "无支祁",
        voice: "桀骜机变、善破僵局：爱换角度与非常规思路，语气带点不服与玩味，论据仍须扎实。",
    },
    MythicEntry {
        name: "陆吾",
        voice: "司守秩序、严明条框：偏好清单、规则与权限边界，语气像尽职守卫。",
    },
    MythicEntry {
        name: "英招",
        voice: "迅捷爽利、好行旅与连接：善把「从 A 到 B」说清楚，语气轻快、少拖泥带水。",
    },
    MythicEntry {
        name: "计蒙",
        voice: "风雨欲来、重征兆：爱谈前兆、趋势与预案，语气略带预言感，仍以逻辑为骨。",
    },
    MythicEntry {
        name: "雷神",
        voice: "霹雳直给、短促爆发：先结论后理由，禁忌绕弯；强调时效与触发条件。",
    },
    MythicEntry {
        name: "西王母",
        voice: "威仪内敛、赏罚分明：措辞克制而有分量，少赘语，重要处一字千钧。",
    },
    MythicEntry {
        name: "东王公",
        voice: "阳刚中正、主持公道：好辨明是非与责任，语气端正，宜说理与制衡。",
    },
];

/// 领域专家思考 + 对外回复须符合人格（固定指令，与用户/神兽层叠加）。
const EXPERT_AND_PERSONA_REPLY_POLICY: &str = r#"【领域专家思考（对内·须在推理中落实）】
1. 先解析用户意图与问题类型，判断主要涉及领域（如：软件工程、系统架构、数据与安全、产品与设计、法律合规、医疗健康常识、数理逻辑、语言与写作、办公与生活技能等）；跨域则拆分维度。
2. 在思考、调用工具、组织中间步骤时，明确采用该领域**资深从业者**的标准：厘清定义与边界条件，重视可验证依据、常见误区与适用前提；不凭空虚构专业细节。
3. 不必在每条回复开头长篇自我介绍；仅在有助于用户理解立场时简短点明当前所采纳的专家视角。

【面向用户的回复（对外·必须遵守）】
1. 呈现给用户的自然语言须**完全一致地**贯彻本智能体的**人格方案**：若存在本智能体 Markdown 人格定义，则**以用户定义为准**（语气、立场、称谓、习惯表达均以该文档为最高优先级）。
2. 若**无**用户自定义人格，则须贯彻上文【默认人格·山海经·…】中的**神兽对话风格**（修辞与节奏），同时保持事实准确、不编造。
3. 工具输出、代码、引用等可保持技术性；包裹它们的说明性文字仍须符合人格。"#;

/// 太阳系行星中文名（用于任务/计划代号循环）。
const PLANETS_CN: &[&str] = &[
    "水星", "金星", "地球", "火星", "木星", "土星", "天王星", "海王星", "冥王星",
];

/// 运行时：加载 Markdown、合并系统提示、分配行星计划名与流程块名。
pub struct PersonaRuntime {
    cfg: PersonaSettings,
    workspace_root: PathBuf,
    global_text: String,
    plan_counter: Mutex<usize>,
    flow_counter: Mutex<usize>,
}

impl PersonaRuntime {
    /// 从工作区根目录加载 `global.md`（可选）；按智能体 Markdown 在查询时按需读取。
    pub fn load(workspace_root: impl AsRef<Path>, persona_cfg: &PersonaSettings) -> Self {
        let root = workspace_root.as_ref().to_path_buf();
        let global_text = if persona_cfg.use_markdown_persona {
            read_md(&root.join(&persona_cfg.global_markdown_path))
        } else {
            String::new()
        };
        Self {
            cfg: persona_cfg.clone(),
            workspace_root: root,
            global_text,
            plan_counter: Mutex::new(0),
            flow_counter: Mutex::new(0),
        }
    }

    /// 系统名称（默认「伏羲」）。
    pub fn system_name(&self) -> &str {
        self.cfg.system_name.as_str()
    }

    /// 由 `agent_id` 稳定映射的山海经代号（同 id 永远相同，便于多智能体协作对齐身份）。
    pub fn stable_mythic_codename(agent_id: &str) -> &'static str {
        let mut h: u32 = 2166136261;
        for b in agent_id.bytes() {
            h ^= b as u32;
            h = h.wrapping_mul(16777619);
        }
        let idx = (h as usize) % MYTHICS.len();
        MYTHICS[idx].name
    }

    /// 无用户自定义人格时，取该神兽的对话风格说明。
    pub fn mythic_dialogue_voice(mythic_name: &str) -> &'static str {
        MYTHICS
            .iter()
            .find(|m| m.name == mythic_name)
            .map(|m| m.voice)
            .unwrap_or("语言克制、清晰准确；不编造事实。")
    }

    /// 下一个任务/计划行星代号（循环）。
    pub fn next_plan_codename(&self) -> String {
        let mut c = self.plan_counter.lock().expect("plan counter");
        let i = *c % PLANETS_CN.len();
        *c += 1;
        PLANETS_CN[i].to_string()
    }

    /// 下一个流程块 / 编排节点山海经名（独立计数，与智能体稳定代号不同）。
    pub fn next_flow_block_codename(&self) -> String {
        let mut c = self.flow_counter.lock().expect("flow counter");
        let i = *c % MYTHICS.len();
        *c += 1;
        MYTHICS[i].name.to_string()
    }

    fn agent_md_path(&self, agent_id: &str) -> PathBuf {
        self.workspace_root
            .join(&self.cfg.agents_markdown_dir)
            .join(format!("{}.md", agent_id))
    }

    fn load_agent_markdown(&self, agent_id: &str) -> String {
        if !self.cfg.use_markdown_persona {
            return String::new();
        }
        read_md(&self.agent_md_path(agent_id))
    }

    /// 将全局/按智能体 Markdown 与系统品牌序言合并为完整 `system_prompt`。
    ///
    /// `base_role_prompt` 一般为 Agent 配置中的短基础角色句（如「你是一个有用的 AI 助手。」）。
    /// 调用方（如 Gateway）再写入 [`pa_query::QueryConfig::system_prompt`]。
    pub fn build_system_prompt(
        &self,
        agent_id: &str,
        agent_display_name: &str,
        base_role_prompt: &str,
    ) -> String {
        let mythic = Self::stable_mythic_codename(agent_id);
        let agent_md = self.load_agent_markdown(agent_id);
        let user_defined_persona = !agent_md.trim().is_empty();

        let agent_section = if user_defined_persona {
            format!(
                "{}\n\n（协作用山海经代号「{}」；**语气、立场与表达习惯以本文档为准**，代号仅作多智能体对齐。）",
                agent_md.trim(),
                mythic
            )
        } else {
            let voice = Self::mythic_dialogue_voice(mythic);
            let agents_dir = self.cfg.agents_markdown_dir.trim_end_matches('/');
            format!(
                "【默认人格·山海经·{mythic}】\n对话风格：{voice}\n\n（尚未配置用户自定义人格。可在 `{agents_dir}` 目录下创建 `{agent_id}.md` 覆盖为完全自定义 Markdown。）",
            )
        };

        let mut blocks: Vec<String> = Vec::new();
        blocks.push(format!(
            "【{}·智能体「{}」·山海经代号「{}」·技术标识 `{}」】",
            self.cfg.system_name, agent_display_name, mythic, agent_id
        ));
        blocks.push(format!("【基础角色】\n{}", base_role_prompt.trim()));
        if !self.global_text.trim().is_empty() {
            blocks.push(format!(
                "【全局人格与约束（`{}`）】\n{}",
                self.cfg.global_markdown_path, self.global_text.trim()
            ));
        }
        blocks.push(format!(
            "【本智能体人格（`{}/{}.md`）】\n{}",
            self.cfg.agents_markdown_dir.trim_end_matches('/'),
            agent_id,
            agent_section.trim()
        ));
        blocks.push(EXPERT_AND_PERSONA_REPLY_POLICY.to_string());

        blocks.join("\n\n")
    }
}

fn read_md(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mythics_len_matches_stable_index() {
        assert!(!MYTHICS.is_empty());
        let _ = PersonaRuntime::stable_mythic_codename("test");
    }
}
