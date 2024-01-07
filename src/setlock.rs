use std::{error::Error, fmt::Display};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SetLockError {
    AlreadySet,
}

impl Display for SetLockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SetLockError::AlreadySet => write!(f, "AlreadySet"),
        }
    }
}

impl Error for SetLockError {}

pub struct SetLock<T> {
    data: Option<T>,
}

impl<T> SetLock<T> {
    pub fn new() -> Self {
        Self { data: None }
    }

    pub fn set(&mut self, data: T) -> Result<(), SetLockError> {
        if self.data.is_some() {
            return Err(SetLockError::AlreadySet);
        }

        self.data = Some(data);

        Ok(())
    }

    pub fn is_set(&self) -> bool {
        self.data.is_some()
    }

    pub fn unwrap(&self) -> &T {
        self.data.as_ref().unwrap()
    }

    pub fn unwrap_mut(&mut self) -> &mut T {
        self.data.as_mut().unwrap()
    }

    pub fn get(&self) -> Option<&T> {
        self.data.as_ref()
    }

    pub fn get_mut(&mut self) -> Option<&mut T> {
        self.data.as_mut()
    }
}

impl<T> Default for SetLock<T> {
    fn default() -> Self {
        Self::new()
    }
}
