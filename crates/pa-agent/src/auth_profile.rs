//! 认证配置管理

/// 认证配置类型
#[derive(Debug, Clone)]
pub enum AuthType {
    ApiKey { key: String },
    OAuth { access_token: String, refresh_token: Option<String>, expires_at: Option<i64> },
}

/// 认证配置
#[derive(Debug, Clone)]
pub struct AuthProfile {
    pub name: String,
    pub provider: String,
    pub auth: AuthType,
    pub priority: u32,
    pub is_healthy: bool,
    pub cooldown_until: Option<i64>,
}

/// 认证配置管理器
pub struct AuthProfileManager {
    profiles: Vec<AuthProfile>,
    current_index: usize,
}

impl AuthProfileManager {
    pub fn new() -> Self {
        Self {
            profiles: Vec::new(),
            current_index: 0,
        }
    }

    /// 添加认证配置
    pub fn add_profile(&mut self, profile: AuthProfile) {
        self.profiles.push(profile);
        // 按优先级排序
        self.profiles.sort_by(|a, b| b.priority.cmp(&a.priority));
    }

    /// 获取当前可用的认证配置
    pub fn get_active(&mut self) -> Option<&AuthProfile> {
        let now = chrono::Utc::now().timestamp();
        
        for (i, profile) in self.profiles.iter().enumerate() {
            if profile.is_healthy {
                if let Some(cooldown) = profile.cooldown_until {
                    if now < cooldown {
                        continue;
                    }
                }
                self.current_index = i;
                return Some(profile);
            }
        }
        None
    }

    /// 标记当前配置为失败（触发冷却）
    pub fn mark_failed(&mut self, duration_secs: i64) {
        if let Some(profile) = self.profiles.get_mut(self.current_index) {
            profile.cooldown_until = Some(chrono::Utc::now().timestamp() + duration_secs);
        }
    }

    /// 切换到下一个配置
    pub fn rotate(&mut self) -> Option<&AuthProfile> {
        self.current_index = (self.current_index + 1) % self.profiles.len().max(1);
        self.get_active()
    }
}
