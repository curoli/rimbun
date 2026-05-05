#[derive(Debug, Clone, Copy)]
pub struct Pagination {
    pub limit: i64,
    pub offset: i64,
}

impl Default for Pagination {
    fn default() -> Self {
        Self {
            limit: 20,
            offset: 0,
        }
    }
}
