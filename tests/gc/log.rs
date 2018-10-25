use std::cell::RefCell;

pub(crate) trait HasLog {
    fn log(&self) -> &Log;
}

#[derive(Default)]
pub(crate) struct Log {
    data: RefCell<Vec<String>>,
}

impl Log {
    pub(crate) fn add(&self, text: impl Into<String>) {
        self.data.borrow_mut().push(text.into());
    }

    pub(crate) fn take(&self) -> Vec<String> {
        std::mem::replace(&mut *self.data.borrow_mut(), vec![])
    }
}
