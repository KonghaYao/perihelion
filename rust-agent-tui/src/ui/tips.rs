/// Pick a tip based on a tick counter. Tip changes every ~180 ticks (roughly every 3 seconds at 60fps).
pub fn pick_tip(tick: u64, lc: &crate::i18n::LcRegistry) -> String {
    let idx = ((tick / 180) as usize) % 18;
    lc.tr(&format!("tip-{}", idx))
}

#[cfg(test)]
mod tests {
    use super::*;
    include!("tips_test.rs");
}
