//! 环境变量替换

use std::env;

/// 环境变量替换器
pub struct EnvSubstitution;

impl EnvSubstitution {
    /// 替换字符串中的环境变量引用
    /// 
    /// 支持格式: ${VAR_NAME} 或 ${VAR_NAME:-default}
    pub fn substitute(content: &str) -> String {
        let mut result = content.to_string();
        
        // 查找所有 ${...} 模式
        let mut start = 0;
        while let Some(pos) = result[start..].find("${") {
            let abs_pos = start + pos;
            if let Some(end) = result[abs_pos..].find('}') {
                let var_part = &result[abs_pos + 2..abs_pos + end];
                let replacement = Self::resolve_var(var_part);
                result.replace_range(abs_pos..abs_pos + end + 1, &replacement);
                start = abs_pos + replacement.len();
            } else {
                break;
            }
        }
        
        result
    }

    fn resolve_var(var_part: &str) -> String {
        // 检查是否有默认值
        if let Some(colon_pos) = var_part.find(":-") {
            let var_name = &var_part[..colon_pos];
            let default = &var_part[colon_pos + 2..];
            env::var(var_name).unwrap_or_else(|_| default.to_string())
        } else {
            env::var(var_part).unwrap_or_default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_substitute() {
        std::env::set_var("TEST_VAR", "test_value");
        let result = EnvSubstitution::substitute("key = \"${TEST_VAR}\"");
        assert_eq!(result, "key = \"test_value\"");
    }
    
    #[test]
    fn test_substitute_with_default() {
        let result = EnvSubstitution::substitute("key = \"${NONEXISTENT:-default_value}\"");
        assert_eq!(result, "key = \"default_value\"");
    }
}
