#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Origin {
    // opaque token identifying our ext_data_control_source
    id: u32,
}

impl Origin {
    pub fn new(id: u32) -> Self {
        Self { id }
    }

    pub fn id(&self) -> u32 {
        self.id
    }
}
