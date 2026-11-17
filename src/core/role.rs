//! 系统角色：从系统标识推断，也允许按快捷方式手动指定。
//!
//! 推断规则是「包含」而不是「首字母」——`ACD`、`QAD` 这类也能命中。代价是误判变多，
//! 所以手动指定是配套的纠正手段。
//!
//! 多个字母同时出现时按 **P > Q > D** 取：宁可把开发显示成生产色，
//! 也不要把生产显示成开发色。

/// 系统角色。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SystemRole {
    Dev,
    Qas,
    Prd,
    Other,
}

impl SystemRole {
    /// 按系统标识推断角色。
    pub fn detect(systemid: &str) -> Self {
        let upper = systemid.to_ascii_uppercase();
        if upper.contains('P') {
            Self::Prd
        } else if upper.contains('Q') {
            Self::Qas
        } else if upper.contains('D') {
            Self::Dev
        } else {
            Self::Other
        }
    }

    /// 存进 `settings.toml` 时用的键。
    pub fn key(self) -> &'static str {
        match self {
            Self::Dev => "dev",
            Self::Qas => "qas",
            Self::Prd => "prd",
            Self::Other => "other",
        }
    }

    /// 界面上的四个可选项（键 + 中文名），手动指定角色时用。
    pub fn choices() -> [(Self, &'static str); 4] {
        [
            (Self::Dev, "dev"),
            (Self::Qas, "qas"),
            (Self::Prd, "prd"),
            (Self::Other, "other"),
        ]
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "dev" => Some(Self::Dev),
            "qas" => Some(Self::Qas),
            "prd" => Some(Self::Prd),
            "other" => Some(Self::Other),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detection_prefers_production() {
        assert_eq!(SystemRole::detect("DS4"), SystemRole::Dev);
        assert_eq!(SystemRole::detect("QS4"), SystemRole::Qas);
        assert_eq!(SystemRole::detect("PS4"), SystemRole::Prd);
        assert_eq!(SystemRole::detect("XYZ"), SystemRole::Other);
        // 包含而非首字母
        assert_eq!(SystemRole::detect("ACD"), SystemRole::Dev);
        // 多字母按 P > Q > D
        assert_eq!(SystemRole::detect("QAD"), SystemRole::Qas);
        assert_eq!(SystemRole::detect("QPD"), SystemRole::Prd);
    }
}
