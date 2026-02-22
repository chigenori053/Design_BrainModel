#[derive(Clone, Debug, PartialEq)]
pub struct Hypothesis {
    pub id: String,
    pub content: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Score(pub f64);
