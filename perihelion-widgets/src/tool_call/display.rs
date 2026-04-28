use super::ToolCallStatus;

pub fn format_indicator(status: ToolCallStatus, tick: u64) -> &'static str {
    match status {
        ToolCallStatus::Pending => "●",
        ToolCallStatus::Running => {
            if (tick / 4) % 2 == 0 {
                "●"
            } else {
                " "
            }
        }
        ToolCallStatus::Completed => "●",
        ToolCallStatus::Failed => "✗",
    }
}

pub fn format_args_summary(args: &str, max_width: usize) -> String {
    if args.len() <= max_width {
        args.to_string()
    } else {
        let mut truncated: String = args.chars().take(max_width.saturating_sub(1)).collect();
        truncated.push('…');
        truncated
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_indicator_running_blinks() {
        assert_eq!(format_indicator(ToolCallStatus::Running, 0), "●");
        assert_eq!(format_indicator(ToolCallStatus::Running, 4), " ");
    }
}
