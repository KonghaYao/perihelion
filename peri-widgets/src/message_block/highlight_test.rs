	    #[test]
	    fn test_highlight_diff_add() {
	        let theme = crate::theme::DarkTheme;
	        let spans = highlight_diff_line("+ added line", &theme);
	        assert!(!spans.is_empty());
	        assert_eq!(spans[0].style.fg, Some(theme.diff_add()));
	    }

	    #[test]
	    fn test_highlight_diff_remove() {
	        let theme = crate::theme::DarkTheme;
	        let spans = highlight_diff_line("- removed line", &theme);
	        assert!(!spans.is_empty());
	        assert_eq!(spans[0].style.fg, Some(theme.diff_remove()));
	    }

	    #[test]
	    fn test_highlight_diff_hunk() {
	        let theme = crate::theme::DarkTheme;
	        let spans = highlight_diff_line("@@ -1,3 +1,4 @@", &theme);
	        assert!(!spans.is_empty());
	        assert_eq!(spans[0].style.fg, Some(theme.diff_hunk()));
	    }

	    #[test]
	    fn test_is_diff_true() {
	        let text = "some line\n@@ -1,3 +1,4 @@\n+ added";
	        assert!(is_diff_content(text));
	    }

	    #[test]
	    fn test_is_diff_false() {
	        let text = "fn main() {\n    println!(\"hello\");\n}";
	        assert!(!is_diff_content(text));
	    }
