//! Shared catalogue search and workspace-only recent choices.
use super::{
    catalog::{self, Category, LibraryItem},
    *,
};
use std::sync::OnceLock;

const RECENT_LIMIT: usize = 16;

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
#[serde(transparent)]
pub struct RecentNodes(Vec<String>);

impl<'de> serde::Deserialize<'de> for RecentNodes {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = RecentNodes;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a list of recent node catalogue keys")
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<Self::Value, A::Error> {
                let mut keys = Vec::new();
                while let Some(key) = seq.next_element::<String>()? {
                    if keys.len() < RECENT_LIMIT
                        && key.len() <= 96
                        && entries().iter().any(|e| e.key == key)
                        && !keys.contains(&key)
                    {
                        keys.push(key);
                    }
                }
                Ok(RecentNodes(keys))
            }
        }
        deserializer.deserialize_seq(Visitor)
    }
}

impl RecentNodes {
    pub(super) fn record(&mut self, key: &str) -> bool {
        if self.0.first().is_some_and(|first| first == key) {
            return false;
        }
        self.0.retain(|old| old != key);
        self.0.insert(0, key.to_owned());
        self.0.truncate(RECENT_LIMIT);
        true
    }
    pub(super) fn items(&self) -> Vec<&'static Entry> {
        self.0
            .iter()
            .filter_map(|key| entries().iter().find(|e| &e.key == key))
            .collect()
    }
}

pub(super) struct Entry {
    pub item: LibraryItem,
    pub key: String,
    titles: [String; 2],
    descriptions: [&'static str; 2],
    aliases: String,
    signatures: [String; 2],
}

fn index(language: Language) -> usize {
    usize::from(language == Language::English)
}

impl Entry {
    pub fn title(&self, language: Language) -> &str {
        &self.titles[index(language)]
    }
    pub fn description(&self, language: Language) -> &str {
        self.descriptions[index(language)]
    }
    pub fn signature(&self, language: Language) -> &str {
        &self.signatures[index(language)]
    }

    fn score(&self, tokens: &[String]) -> Option<usize> {
        let mut score = 0;
        for token in tokens {
            let rank = self
                .titles
                .iter()
                .map(|title| {
                    let title = title.to_lowercase();
                    if title == *token {
                        100
                    } else if title.starts_with(token) {
                        80
                    } else if title.contains(token) {
                        60
                    } else {
                        0
                    }
                })
                .max()
                .unwrap_or(0);
            let rank = rank
                .max(if self.item.operation.kind().contains(token) {
                    45
                } else {
                    0
                })
                .max(if self.aliases.contains(token) { 35 } else { 0 })
                .max(
                    if self
                        .descriptions
                        .iter()
                        .any(|s| s.to_lowercase().contains(token))
                    {
                        15
                    } else {
                        0
                    },
                );
            if rank == 0 {
                return None;
            }
            score += rank;
        }
        Some(score)
    }
}

pub(super) fn entries() -> &'static [Entry] {
    static ENTRIES: OnceLock<Vec<Entry>> = OnceLock::new();
    ENTRIES.get_or_init(|| {
        catalog()
            .into_iter()
            .map(|item| {
                let titles = [item.title(Language::Chinese), item.title(Language::English)];
                let key = format!(
                    "{}:{}",
                    item.operation.kind(),
                    titles[1]
                        .chars()
                        .filter(char::is_ascii_alphanumeric)
                        .flat_map(char::to_lowercase)
                        .collect::<String>()
                );
                let descriptions = if item.parameter_type.is_some() {
                    (
                        "将可编辑参数公开给效果使用者。",
                        "Expose an editable control to users of the effect.",
                    )
                } else {
                    motion_graph::node_description(&item.operation)
                };
                let signatures = if let Some(ty) = item.parameter_type {
                    [
                        format!("无输入 → 值: {}", catalog::type_label(ty)),
                        format!("No inputs → Value: {}", catalog::type_label(ty)),
                    ]
                } else {
                    match motion_graph::node_port_specs(
                        &Node::new("catalogue", item.operation.clone()),
                        &[],
                    ) {
                        Ok(specs) => [Language::Chinese, Language::English].map(|language| {
                            let ports = |ports: &BTreeMap<String, motion_graph::PortSpec>| {
                                ports
                                    .values()
                                    .map(|p| {
                                        format!(
                                            "{}: {}",
                                            language.text(p.name, p.english_name),
                                            catalog::type_label(p.value_type)
                                        )
                                    })
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            };
                            format!(
                                "{} → {}",
                                if specs.inputs.is_empty() {
                                    language.text("无输入", "No inputs").into()
                                } else {
                                    ports(&specs.inputs)
                                },
                                ports(&specs.outputs)
                            )
                        }),
                        Err(error) => {
                            log::error!(
                                "node catalogue metadata failed operation={} reason={error}",
                                item.operation.kind()
                            );
                            [
                                "端口信息不可用".into(),
                                "Port information unavailable".into(),
                            ]
                        }
                    }
                };
                let aliases = aliases(&item);
                Entry {
                    item,
                    key,
                    titles,
                    descriptions: [descriptions.0, descriptions.1],
                    aliases,
                    signatures,
                }
            })
            .collect()
    })
}

/// Queries deliberately cross category boundaries; blank browsing keeps category order.
pub(super) fn find(query: &str, category: Option<Category>) -> Vec<&'static Entry> {
    let tokens: Vec<_> = query
        .split_whitespace()
        .map(str::to_lowercase)
        .collect();
    let mut matches: Vec<_> = entries()
        .iter()
        .enumerate()
        .filter_map(|(order, entry)| {
            if tokens.is_empty() && category.is_some_and(|c| c != entry.item.category) {
                return None;
            }
            entry.score(&tokens).map(|score| (score, order, entry))
        })
        .collect();
    matches.sort_by_key(|(score, order, _)| (std::cmp::Reverse(*score), *order));
    matches.into_iter().map(|(_, _, e)| e).collect()
}

fn aliases(item: &LibraryItem) -> String {
    let words = if item.parameter_type.is_some() {
        "gongkaicanshu gkcs parameter public control 参数"
    } else {
        match &item.operation {
            NodeOp::Position => "weizhi wz coordinate position 坐标",
            NodeOp::Uv => "uv zuobiao zb normalized coordinates 归一化坐标",
            NodeOp::Time => "shijian sj seconds 秒",
            NodeOp::FrameIndex => "zhenshu zs frame index 帧数",
            NodeOp::Resolution => "fencun fenbianlv fbl resolution dimensions 尺寸 分辨率",
            NodeOp::Image {
                source: ImageSource::Input,
            } => "tucengshuru tcsr texture 图层输入 纹理",
            NodeOp::Image {
                source: ImageSource::Source,
            } => "yuanshituxiang ystx original texture 原始图像 纹理",
            NodeOp::SampleImage { .. } => "caiyangtuxiang cytx sample texture 采样图像 纹理",
            NodeOp::Float { .. } => "fudianshu fds scalar number constant 标量 数值 常量",
            NodeOp::Vec2 { .. } => "erweixiangliang ewxl vector constant 二维向量 常量",
            NodeOp::Vec3 { .. } => "sanweixiangliang swxl vector constant 三维向量 常量",
            NodeOp::Color { .. } => "yanse ys rgba rgb colour constant 颜色 常量",
            NodeOp::Bool { .. } => "kaiguan kg boolean bool constant 开关 布尔 常量",
            NodeOp::Math { operation, .. } => match operation {
                MathOp::Add => "xiangjia xj sum plus + 加法",
                MathOp::Subtract => "xiangjian xj minus subtraction 减法",
                MathOp::Multiply => "xiangcheng xc product times multiplication 乘法",
                MathOp::Divide => "xiangchu xc quotient division 除法",
                MathOp::Min => "zuixiaozhi zxz min 最小",
                MathOp::Max => "zuidazhi zdz max 最大",
                MathOp::Abs => "jueduizhi jdz abs 绝对值",
                MathOp::Sin => "zhengxian zx sin 正弦",
                MathOp::Cos => "yuxian yx cos 余弦",
                MathOp::Sqrt => "pingfanggen pfg sqrt 平方根",
                MathOp::Floor => "xiangxiaquzheng xxqz floor 向下取整",
                MathOp::Fract => "xiaoshubufen xsbf fract 小数",
                MathOp::Clamp => "xianzhifanwei xzfw clamp limit 限制 钳制",
                MathOp::Lerp => "xianxingchazhi xxcz interpolate lerp 插值",
                MathOp::Smoothstep => "pinghuajieyue phjy smoothstep 平滑阶跃",
                MathOp::Dot => "dianji dj dot product 点积",
                MathOp::Length => "changdu cd length magnitude 长度 模长",
                MathOp::Normalize => "guiyihua gyh normalize unit 归一化 单位",
            },
            NodeOp::Splat { .. } => "fuzhi fz splat scalar vector broadcast 标量 向量 复制",
            NodeOp::CombineVec2 | NodeOp::CombineVec3 => {
                "hechengxiangliang hcxl combine vector xy xyz 合成向量"
            }
            NodeOp::Select { .. } => "tiaojianxuanze tjxz select conditional 条件选择",
            NodeOp::CombineColor => "hechengyanse hcys combine rgba rgb 合成颜色",
            NodeOp::SplitColor => "chaifenyanse cfys split separate rgba 拆分颜色",
            NodeOp::ApplyMask => "yingyongmengban yymb apply mask alpha 蒙版 透明度",
            NodeOp::ColorMath { operation } => match operation {
                ColorOp::Invert => "fanxiang fx invert negative 反相 反色",
                ColorOp::Mix => "yansehunhe yshh mix blend 颜色混合",
                ColorOp::Multiply => "zhengpiandiedi zpdd multiply blend 正片叠底",
                ColorOp::Screen => "lvse ls screen blend 滤色",
                ColorOp::SrgbEncode => "bianma bm srgb encode linear 编码",
                ColorOp::SrgbDecode => "jiema jm srgb decode linear 解码",
            },
            NodeOp::SdfCircle => "yuanxing yx circle disk shape 圆形",
            NodeOp::SdfRoundedRectangle => {
                "yuanjiaojuxing yjjx rounded rectangle box shape 圆角矩形"
            }
            NodeOp::SdfLine => "xianduan xd line segment shape 线段",
            NodeOp::SdfCapsule => "jiaonang jn capsule shape 胶囊",
            NodeOp::SdfUnion => "bingji bj union merge 并集",
            NodeOp::SdfIntersection => "jiaoji jj intersection 交集",
            NodeOp::SdfDifference => "chaji cj difference subtract 差集",
            NodeOp::SdfSmoothUnion => "pinghuabingji phbj smooth union 平滑并集",
            NodeOp::SdfFill => "tianchong tc fill coverage 填充",
            NodeOp::SdfStroke => "miaobian mb stroke outline 描边",
            NodeOp::ColorOutput => "yanseshuchu yssc output result 颜色输出",
            _ => match item.operation.kind() {
                "gradient" => "jianbian jb linear radial conic 渐变",
                "noise" => "zaosheng zs value perlin 噪声",
                "blend" => "hunhe hh blend composite 混合 合成",
                _ => "",
            },
        }
    };
    // A category can contain unrelated operations (for example constants and
    // public parameters). Its label must not turn every member into an alias.
    words.to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn query_crosses_categories_and_requires_every_token() {
        assert!(find("circle", Some(Category::Math))
            .iter()
            .any(|e| matches!(e.item.operation, NodeOp::SdfCircle)));
        assert!(find("", Some(Category::Math))
            .iter()
            .all(|e| e.item.category == Category::Math));
        assert!(find("   ", Some(Category::Input))
            .iter()
            .all(|e| e.item.category == Category::Input));
        assert!(find("vec2 parameter", None)
            .iter()
            .all(|e| e.item.parameter_type == Some(ValueType::Vec2)));
        assert!(find("circle no_such_node", None).is_empty());
        assert!(find(&format!("{} no_such_node", "circle ".repeat(16)), None).is_empty());
    }
    #[test]
    fn bilingual_aliases_pinyin_and_rank_are_stable() {
        for query in ["圆形", "circle", "yuanxing", "yx shape"] {
            assert!(
                matches!(find(query, None)[0].item.operation, NodeOp::SdfCircle),
                "{query}"
            );
        }
        assert!(matches!(
            find("sqrt", None)[0].item.operation,
            NodeOp::Math {
                operation: MathOp::Sqrt,
                ..
            }
        ));
        assert!(matches!(
            find("Float", None)[0].item.operation,
            NodeOp::Float { .. }
        ));
        assert!(find("seconds", None)
            .iter()
            .any(|e| matches!(e.item.operation, NodeOp::Time)));
    }
    #[test]
    fn all_catalogue_entries_have_unique_keys_help_and_real_signatures() {
        let mut keys = BTreeSet::new();
        for e in entries() {
            assert!(keys.insert(e.key.clone()), "duplicate {}", e.key);
            assert!(e.key.len() <= 96 && e.key.is_ascii());
            for language in [Language::Chinese, Language::English] {
                assert!(!e.description(language).is_empty());
                assert!(e.signature(language).contains('→'));
                assert!(!e.signature(language).contains("unavailable"));
            }
        }
    }
    #[test]
    fn recents_bound_deduplicate_reorder_and_sanitize_saved_keys() {
        let mut recent = RecentNodes::default();
        for e in entries() {
            recent.record(&e.key);
        }
        assert_eq!(recent.items().len(), RECENT_LIMIT);
        let first = entries()[0].key.clone();
        assert!(recent.record(&first));
        assert!(!recent.record(&first));
        assert_eq!(recent.items()[0].key, first);
        let serialized = serde_json::to_string(&recent).unwrap();
        assert_eq!(
            serde_json::from_str::<RecentNodes>(&serialized).unwrap(),
            recent
        );
        let malformed = serde_json::json!([first, "missing:key", first, "x".repeat(200)]);
        let clean: RecentNodes = serde_json::from_value(malformed).unwrap();
        assert_eq!(clean.items().len(), 1);
    }
}
