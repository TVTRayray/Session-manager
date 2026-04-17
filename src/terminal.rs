use std::io;

use crossterm::event::{
    DisableMouseCapture, EnableMouseCapture, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags,
    PushKeyboardEnhancementFlags,
};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};

pub trait TerminalOps {
    fn enable_raw_mode(&mut self) -> io::Result<()>;
    fn disable_raw_mode(&mut self) -> io::Result<()>;
    fn enter_alternate_screen(&mut self) -> io::Result<()>;
    fn leave_alternate_screen(&mut self) -> io::Result<()>;
    fn push_keyboard_enhancement_flags(&mut self) -> io::Result<()>;
    fn pop_keyboard_enhancement_flags(&mut self) -> io::Result<()>;
}

#[derive(Default)]
pub struct CrosstermTerminalOps;

impl TerminalOps for CrosstermTerminalOps {
    fn enable_raw_mode(&mut self) -> io::Result<()> {
        enable_raw_mode()
    }

    fn disable_raw_mode(&mut self) -> io::Result<()> {
        disable_raw_mode()
    }

    fn enter_alternate_screen(&mut self) -> io::Result<()> {
        execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture)
    }

    fn leave_alternate_screen(&mut self) -> io::Result<()> {
        execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture)
    }

    fn push_keyboard_enhancement_flags(&mut self) -> io::Result<()> {
        execute!(
            io::stdout(),
            PushKeyboardEnhancementFlags(
                KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                    | KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS
            )
        )
    }

    fn pop_keyboard_enhancement_flags(&mut self) -> io::Result<()> {
        execute!(io::stdout(), PopKeyboardEnhancementFlags)
    }
}

pub struct TerminalModeGuard<O: TerminalOps> {
    ops: O,
    raw_mode_enabled: bool,
    alternate_screen_enabled: bool,
    keyboard_enhancement_enabled: bool,
}

impl<O: TerminalOps> TerminalModeGuard<O> {
    pub fn activate(mut ops: O) -> io::Result<Self> {
        ops.enable_raw_mode()?;

        let mut guard = Self {
            ops,
            raw_mode_enabled: true,
            alternate_screen_enabled: false,
            keyboard_enhancement_enabled: false,
        };

        if let Err(err) = guard.ops.enter_alternate_screen() {
            let _ = guard.ops.disable_raw_mode();
            guard.raw_mode_enabled = false;
            return Err(err);
        }

        guard.alternate_screen_enabled = true;
        if let Err(err) = guard.ops.push_keyboard_enhancement_flags() {
            let _ = guard.ops.leave_alternate_screen();
            let _ = guard.ops.disable_raw_mode();
            guard.alternate_screen_enabled = false;
            guard.raw_mode_enabled = false;
            return Err(err);
        }

        guard.keyboard_enhancement_enabled = true;
        Ok(guard)
    }

    pub fn restore(&mut self) -> io::Result<()> {
        let mut first_error = None;

        if self.keyboard_enhancement_enabled {
            if let Err(err) = self.ops.pop_keyboard_enhancement_flags() {
                first_error = Some(err);
            }
            self.keyboard_enhancement_enabled = false;
        }

        if self.alternate_screen_enabled {
            if let Err(err) = self.ops.leave_alternate_screen() {
                if first_error.is_none() {
                    first_error = Some(err);
                }
            }
            self.alternate_screen_enabled = false;
        }

        if self.raw_mode_enabled {
            if let Err(err) = self.ops.disable_raw_mode() {
                if first_error.is_none() {
                    first_error = Some(err);
                }
            }
            self.raw_mode_enabled = false;
        }

        match first_error {
            Some(err) => Err(err),
            None => Ok(()),
        }
    }
}

impl<O: TerminalOps> Drop for TerminalModeGuard<O> {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Clone, Default)]
    struct MockTerminalOps {
        shared: Rc<RefCell<MockState>>,
    }

    #[derive(Default)]
    struct MockState {
        calls: Vec<&'static str>,
        fail_enter: bool,
        fail_push_keyboard: bool,
    }

    impl MockTerminalOps {
        fn with_fail_enter() -> Self {
            let ops = Self::default();
            ops.shared.borrow_mut().fail_enter = true;
            ops
        }

        fn calls(&self) -> Vec<&'static str> {
            self.shared.borrow().calls.clone()
        }

        fn with_fail_push_keyboard() -> Self {
            let ops = Self::default();
            ops.shared.borrow_mut().fail_push_keyboard = true;
            ops
        }
    }

    impl TerminalOps for MockTerminalOps {
        fn enable_raw_mode(&mut self) -> io::Result<()> {
            self.shared.borrow_mut().calls.push("enable_raw_mode");
            Ok(())
        }

        fn disable_raw_mode(&mut self) -> io::Result<()> {
            self.shared.borrow_mut().calls.push("disable_raw_mode");
            Ok(())
        }

        fn enter_alternate_screen(&mut self) -> io::Result<()> {
            self.shared
                .borrow_mut()
                .calls
                .push("enter_alternate_screen");
            if self.shared.borrow().fail_enter {
                Err(io::Error::other("enter failed"))
            } else {
                Ok(())
            }
        }

        fn leave_alternate_screen(&mut self) -> io::Result<()> {
            self.shared
                .borrow_mut()
                .calls
                .push("leave_alternate_screen");
            Ok(())
        }

        fn push_keyboard_enhancement_flags(&mut self) -> io::Result<()> {
            self.shared
                .borrow_mut()
                .calls
                .push("push_keyboard_enhancement_flags");
            if self.shared.borrow().fail_push_keyboard {
                Err(io::Error::other("push keyboard failed"))
            } else {
                Ok(())
            }
        }

        fn pop_keyboard_enhancement_flags(&mut self) -> io::Result<()> {
            self.shared
                .borrow_mut()
                .calls
                .push("pop_keyboard_enhancement_flags");
            Ok(())
        }
    }

    #[test]
    fn activation_failure_restores_raw_mode() {
        let ops = MockTerminalOps::with_fail_enter();
        let result = TerminalModeGuard::activate(ops.clone());
        assert!(result.is_err());
        assert_eq!(
            ops.calls(),
            vec![
                "enable_raw_mode",
                "enter_alternate_screen",
                "disable_raw_mode"
            ]
        );
    }

    #[test]
    fn keyboard_enhancement_failure_restores_terminal_mode() {
        let ops = MockTerminalOps::with_fail_push_keyboard();
        let result = TerminalModeGuard::activate(ops.clone());
        assert!(result.is_err());
        assert_eq!(
            ops.calls(),
            vec![
                "enable_raw_mode",
                "enter_alternate_screen",
                "push_keyboard_enhancement_flags",
                "leave_alternate_screen",
                "disable_raw_mode"
            ]
        );
    }

    #[test]
    fn restore_leaves_alternate_screen_and_raw_mode() {
        let ops = MockTerminalOps::default();
        let mut guard = match TerminalModeGuard::activate(ops.clone()) {
            Ok(guard) => guard,
            Err(err) => panic!("unexpected error: {err}"),
        };

        let result = guard.restore();
        assert!(result.is_ok());
        assert_eq!(
            ops.calls(),
            vec![
                "enable_raw_mode",
                "enter_alternate_screen",
                "push_keyboard_enhancement_flags",
                "pop_keyboard_enhancement_flags",
                "leave_alternate_screen",
                "disable_raw_mode"
            ]
        );
    }
}
