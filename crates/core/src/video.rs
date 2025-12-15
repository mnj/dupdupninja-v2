use std::path::Path;

use crate::error::Result;

#[derive(Debug, Clone)]
pub struct VideoSignature {
    pub frame_hashes: Vec<[u8; 32]>,
    pub duration_ms: Option<u64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub codec: Option<String>,
}

pub trait VideoAnalyzer {
    fn signature(&self, path: &Path) -> Result<VideoSignature>;
}

