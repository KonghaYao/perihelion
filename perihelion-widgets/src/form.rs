use std::collections::HashMap;
use std::hash::Hash;

use crate::input_field::InputState;

/// 表单字段枚举 trait——由使用方实现
pub trait FormField: Copy + Eq + Hash + 'static {
    fn next(self) -> Self;
    fn prev(self) -> Self;
    fn label(self) -> &'static str;
}

pub struct FormState<F: FormField> {
    active: F,
    fields: HashMap<F, InputState>,
}

impl<F: FormField> FormState<F> {
    pub fn new(fields: impl Iterator<Item = F>) -> Self {
        let map: HashMap<F, InputState> = fields
            .map(|f| (f, InputState::new()))
            .collect();
        let active = map.keys().copied().next().unwrap();
        Self { active, fields: map }
    }

    pub fn with_active(fields: &[F], active: F) -> Self {
        let mut state = Self::new(fields.iter().copied());
        state.active = active;
        state
    }

    pub fn next_field(&mut self) {
        self.active = self.active.next();
    }

    pub fn prev_field(&mut self) {
        self.active = self.active.prev();
    }

    pub fn active_field(&self) -> F { self.active }

    pub fn set_active(&mut self, field: F) { self.active = field; }

    pub fn input(&self, field: F) -> &InputState {
        self.fields.get(&field).expect("FormState: field not found")
    }

    pub fn input_mut(&mut self, field: F) -> &mut InputState {
        self.fields.get_mut(&field).expect("FormState: field not found")
    }

    pub fn active_input(&self) -> &InputState { self.input(self.active) }

    pub fn active_input_mut(&mut self) -> &mut InputState { self.input_mut(self.active) }

    pub fn handle_char(&mut self, c: char) { self.active_input_mut().insert(c); }
    pub fn handle_backspace(&mut self) { self.active_input_mut().backspace(); }
    pub fn handle_delete(&mut self) { self.active_input_mut().delete(); }
    pub fn handle_cursor_left(&mut self) { self.active_input_mut().cursor_left(); }
    pub fn handle_cursor_right(&mut self) { self.active_input_mut().cursor_right(); }
    pub fn handle_cursor_home(&mut self) { self.active_input_mut().cursor_home(); }
    pub fn handle_cursor_end(&mut self) { self.active_input_mut().cursor_end(); }
    pub fn handle_paste(&mut self, text: &str) { self.active_input_mut().paste(text); }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    enum TestField { A, B, C }

    impl FormField for TestField {
        fn next(self) -> Self { match self { Self::A => Self::B, Self::B => Self::C, Self::C => Self::A } }
        fn prev(self) -> Self { match self { Self::A => Self::C, Self::B => Self::A, Self::C => Self::B } }
        fn label(self) -> &'static str { match self { Self::A => "A", Self::B => "B", Self::C => "C" } }
    }

    #[test]
    fn form_state_field_navigation() {
        let fields = [TestField::A, TestField::B, TestField::C];
        let mut state = FormState::with_active(&fields, TestField::A);
        assert_eq!(state.active_field(), TestField::A);
        state.next_field();
        assert_eq!(state.active_field(), TestField::B);
        state.next_field();
        assert_eq!(state.active_field(), TestField::C);
        state.next_field();
        assert_eq!(state.active_field(), TestField::A); // wraps
        state.prev_field();
        assert_eq!(state.active_field(), TestField::C); // wraps back
    }

    #[test]
    fn form_state_text_editing() {
        let mut state = FormState::new([TestField::A, TestField::B, TestField::C].into_iter());
        state.handle_char('h');
        state.handle_char('i');
        assert_eq!(state.active_input().value(), "hi");
        state.handle_backspace();
        assert_eq!(state.active_input().value(), "h");
    }

    #[test]
    fn form_state_independent_fields() {
        let mut state = FormState::new([TestField::A, TestField::B, TestField::C].into_iter());
        state.handle_char('h');
        state.handle_char('i');
        state.next_field();
        state.handle_char('x');
        state.prev_field();
        assert_eq!(state.active_input().value(), "hi");
    }

    #[test]
    fn form_state_cursor_movement() {
        let mut state = FormState::new([TestField::A, TestField::B, TestField::C].into_iter());
        state.handle_char('a');
        state.handle_char('b');
        state.handle_cursor_home();
        state.handle_char('X');
        assert_eq!(state.active_input().value(), "Xab");
    }

    #[test]
    fn form_state_paste() {
        let mut state = FormState::new([TestField::A, TestField::B, TestField::C].into_iter());
        state.handle_paste("hello");
        assert_eq!(state.active_input().value(), "hello");
    }

    #[test]
    fn form_state_set_active() {
        let mut state = FormState::new([TestField::A, TestField::B, TestField::C].into_iter());
        state.set_active(TestField::C);
        assert_eq!(state.active_field(), TestField::C);
        assert_eq!(state.input(TestField::A).value(), "");
    }
}
