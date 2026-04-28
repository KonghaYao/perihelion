use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

/// 权限模式枚举，控制 HITL 审批行为
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PermissionMode {
    /// 所有敏感工具弹窗审批（默认）
    Default = 0,
    /// 默认不允许所有 bash
    DontAsk = 1,
    /// 允许文件系统的编辑
    AcceptEdit = 2,
    /// 大模型自动判断允不允许
    AutoMode = 3,
    /// 所有都允许
    Bypass = 4,
}

impl Default for PermissionMode {
    fn default() -> Self {
        Self::Default
    }
}

impl PermissionMode {
    /// 循环切换到下一个模式：Default → DontAsk → AcceptEdit → AutoMode → Bypass → Default
    pub fn next(self) -> Self {
        match self {
            Self::Default => Self::DontAsk,
            Self::DontAsk => Self::AcceptEdit,
            Self::AcceptEdit => Self::AutoMode,
            Self::AutoMode => Self::Bypass,
            Self::Bypass => Self::Default,
        }
    }

    /// 状态栏显示文本
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Default => "",
            Self::DontAsk => "Don't Ask",
            Self::AcceptEdit => "Accept Edit",
            Self::AutoMode => "Auto Mode",
            Self::Bypass => "Bypass",
        }
    }
}

/// TryFrom<u8> 实现：异常值（>4）回退到 Default
impl TryFrom<u8> for PermissionMode {
    type Error = std::convert::Infallible;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Ok(match value {
            0 => Self::Default,
            1 => Self::DontAsk,
            2 => Self::AcceptEdit,
            3 => Self::AutoMode,
            4 => Self::Bypass,
            _ => Self::Default,
        })
    }
}

/// 跨线程共享的权限模式状态（Arc<AtomicU8> 封装）
pub struct SharedPermissionMode {
    inner: AtomicU8,
}

impl SharedPermissionMode {
    /// 创建新的共享权限模式实例，返回 Arc<Self>
    pub fn new(mode: PermissionMode) -> Arc<Self> {
        Arc::new(Self {
            inner: AtomicU8::new(mode as u8),
        })
    }

    /// 读取当前权限模式
    pub fn load(&self) -> PermissionMode {
        let v = self.inner.load(Ordering::Relaxed);
        PermissionMode::try_from(v).unwrap_or(PermissionMode::Default)
    }

    /// 设置权限模式
    pub fn store(&self, mode: PermissionMode) {
        self.inner.store(mode as u8, Ordering::Relaxed);
    }

    /// CAS 循环切换到下一个模式，返回切换后的模式
    pub fn cycle(&self) -> PermissionMode {
        loop {
            let current = self.inner.load(Ordering::Relaxed);
            let current_mode = PermissionMode::try_from(current).unwrap_or(PermissionMode::Default);
            let next_mode = current_mode.next();
            let next = next_mode as u8;
            match self.inner.compare_exchange(
                current,
                next,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => return next_mode,
                Err(_) => continue,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_next_cycle() {
        assert_eq!(PermissionMode::Default.next(), PermissionMode::DontAsk);
        assert_eq!(PermissionMode::DontAsk.next(), PermissionMode::AcceptEdit);
        assert_eq!(PermissionMode::AcceptEdit.next(), PermissionMode::AutoMode);
        assert_eq!(PermissionMode::AutoMode.next(), PermissionMode::Bypass);
        assert_eq!(PermissionMode::Bypass.next(), PermissionMode::Default);
    }

    #[test]
    fn test_default() {
        assert_eq!(PermissionMode::default(), PermissionMode::Default);
    }

    #[test]
    fn test_display_name() {
        assert_eq!(PermissionMode::Default.display_name(), "");
        assert_eq!(PermissionMode::DontAsk.display_name(), "Don't Ask");
        assert_eq!(PermissionMode::AcceptEdit.display_name(), "Accept Edit");
        assert_eq!(PermissionMode::AutoMode.display_name(), "Auto Mode");
        assert_eq!(PermissionMode::Bypass.display_name(), "Bypass");
    }

    #[test]
    fn test_try_from_u8_valid() {
        assert_eq!(PermissionMode::try_from(0).unwrap(), PermissionMode::Default);
        assert_eq!(PermissionMode::try_from(1).unwrap(), PermissionMode::DontAsk);
        assert_eq!(PermissionMode::try_from(2).unwrap(), PermissionMode::AcceptEdit);
        assert_eq!(PermissionMode::try_from(3).unwrap(), PermissionMode::AutoMode);
        assert_eq!(PermissionMode::try_from(4).unwrap(), PermissionMode::Bypass);
    }

    #[test]
    fn test_try_from_u8_invalid() {
        assert_eq!(PermissionMode::try_from(5).unwrap(), PermissionMode::Default);
        assert_eq!(PermissionMode::try_from(255).unwrap(), PermissionMode::Default);
    }

    #[test]
    fn test_shared_new_and_load() {
        let shared = SharedPermissionMode::new(PermissionMode::AutoMode);
        assert_eq!(shared.load(), PermissionMode::AutoMode);
    }

    #[test]
    fn test_shared_store_and_load() {
        let shared = SharedPermissionMode::new(PermissionMode::Default);
        shared.store(PermissionMode::Bypass);
        assert_eq!(shared.load(), PermissionMode::Bypass);
    }

    #[test]
    fn test_shared_cycle_single_thread() {
        let shared = SharedPermissionMode::new(PermissionMode::Default);
        assert_eq!(shared.cycle(), PermissionMode::DontAsk);
        assert_eq!(shared.cycle(), PermissionMode::AcceptEdit);
        assert_eq!(shared.cycle(), PermissionMode::AutoMode);
        assert_eq!(shared.cycle(), PermissionMode::Bypass);
        assert_eq!(shared.cycle(), PermissionMode::Default);
    }

    #[test]
    fn test_shared_cycle_concurrent() {
        let shared = SharedPermissionMode::new(PermissionMode::Default);
        let shared_clone = shared.clone();
        let barrier = Arc::new(std::sync::Barrier::new(4));

        let mut handles = vec![];
        for _ in 0..4 {
            let s = shared_clone.clone();
            let b = barrier.clone();
            handles.push(thread::spawn(move || {
                b.wait();
                for _ in 0..100 {
                    s.cycle();
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        // 最终状态应为合法 PermissionMode
        let final_mode = shared.load();
        assert!(matches!(
            final_mode,
            PermissionMode::Default
                | PermissionMode::DontAsk
                | PermissionMode::AcceptEdit
                | PermissionMode::AutoMode
                | PermissionMode::Bypass
        ));
    }
}
