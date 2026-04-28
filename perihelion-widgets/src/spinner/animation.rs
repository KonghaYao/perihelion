const BRAILLE_FRAMES: &[char] = &[
    '✳', '✴', '✵', '✶', '✷', '✸', '✹', '✺', '✻', '✼',
    '❃', '❊', '✼', '✻', '✺', '✸',
];

pub fn tick_to_frame(tick: u64) -> char {
    BRAILLE_FRAMES[(tick as usize) % BRAILLE_FRAMES.len()]
}

pub fn smooth_increment(displayed: usize, target: usize) -> usize {
    if displayed >= target {
        return target;
    }
    let gap = target - displayed;
    let step = if gap < 70 {
        3
    } else if gap < 200 {
        (gap * 15 / 100).max(8)
    } else {
        50
    };
    (displayed + step).min(target)
}

pub fn format_elapsed(elapsed_ms: u64) -> String {
    let secs = elapsed_ms / 1000;
    let mins = secs / 60;
    let secs = secs % 60;
    if mins > 0 {
        format!("{}m {}s", mins, secs)
    } else {
        format!("{}s", secs)
    }
}

pub fn format_tokens(count: usize) -> String {
    if count >= 1000 {
        let k = count as f64 / 1000.0;
        if k >= 10.0 {
            format!("{:.0}k", k)
        } else {
            format!("{:.1}k", k)
        }
    } else {
        count.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tick_to_frame_cycle() {
        for i in 0..20 {
            let frame = tick_to_frame(i);
            assert!(
                BRAILLE_FRAMES.contains(&frame),
                "tick {} returned {:?} not in BRAILLE_FRAMES",
                i,
                frame
            );
        }
    }

    #[test]
    fn test_smooth_increment_convergence() {
        let mut displayed = 0;
        let target = 100;
        for _ in 0..200 {
            displayed = smooth_increment(displayed, target);
            if displayed >= target {
                break;
            }
        }
        assert_eq!(displayed, target);
    }

    #[test]
    fn test_format_elapsed() {
        assert_eq!(format_elapsed(90_000), "1m 30s");
        assert_eq!(format_elapsed(30_000), "30s");
        assert_eq!(format_elapsed(5_000), "5s");
    }

    #[test]
    fn test_format_tokens() {
        assert_eq!(format_tokens(500), "500");
        assert_eq!(format_tokens(1500), "1.5k");
        assert_eq!(format_tokens(2200), "2.2k");
        assert_eq!(format_tokens(15000), "15k");
        assert_eq!(format_tokens(0), "0");
    }
}
