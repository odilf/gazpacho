pub struct Signature {
    names: &'static [&'static str],
}

impl Signature {
    pub const fn new(names: &'static [&'static str]) -> Self {
        Self { names }
    }

    pub const fn len(&self) -> usize {
        self.names.len()
    }

    pub const fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.names
            .iter()
            .enumerate()
            .find_map(|(i, &argname)| (argname == name).then_some(i))
    }
}
