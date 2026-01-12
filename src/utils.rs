use std::ops::Deref;

pub struct Handle<T, F>
where
    F: for<'a> Fn(&'a mut T),
{
    handle: T,
    drop_fn: F,
}

impl<T, F> Handle<T, F>
where
    F: for<'a> Fn(&'a mut T),
{
    pub fn new(handle: T, drop_fn: F) -> Self {
        Self { handle, drop_fn }
    }
}

impl<T, F> Drop for Handle<T, F>
where
    F: for<'a> Fn(&'a mut T),
{
    fn drop(&mut self) {
        (self.drop_fn)(&mut self.handle)
    }
}

impl<T, F> Deref for Handle<T, F>
where
    F: for<'a> Fn(&'a mut T),
{
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.handle
    }
}

pub fn normalize(n: u8, min: u8, max: u8) -> u8 {
    let n = n as f32;
    let min = min as f32;
    let max = max as f32;

    let factor = (max - min) / 100.0;
    let n = n * factor + min;

    (n as u8).clamp(0, 100)
}
