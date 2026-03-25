pub struct OptimizationState {
    pub changed: bool,
}

impl OptimizationState {
    #[must_use]
    pub const fn new() -> Self {
        Self { changed: false }
    }
}

impl Default for OptimizationState {
    fn default() -> Self {
        Self::new()
    }
}
