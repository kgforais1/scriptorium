//! A test engine that returns fixed results, so most tests need no engine binary.

use super::engine::{EngineId, Lang, OcrEngine, OcrError, PageOcr};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

/// Returns `result` for every image, after the same checks a real engine makes.
pub struct FakeEngine {
    /// The id it reports.
    pub id: String,
    /// What every call returns once the checks pass.
    pub result: Result<PageOcr, OcrError>,
}

impl OcrEngine for FakeEngine {
    fn id(&self) -> EngineId {
        EngineId(self.id.clone())
    }

    fn recognize(
        &self,
        _image: &Path,
        langs: &[Lang],
        stop: &AtomicBool,
        deadline: Instant,
    ) -> Result<PageOcr, OcrError> {
        if langs.is_empty() {
            return Err(OcrError::NoLanguage);
        }
        if stop.load(Ordering::Relaxed) {
            return Err(OcrError::Cancelled);
        }
        if Instant::now() >= deadline {
            return Err(OcrError::Timeout);
        }
        self.result.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn checks_language_stop_and_deadline() {
        let page = PageOcr {
            width: 1,
            height: 1,
            lines: vec![],
        };
        let e = FakeEngine {
            id: "fake 1".into(),
            result: Ok(page.clone()),
        };
        let later = Instant::now() + Duration::from_secs(60);
        let go = AtomicBool::new(false);
        let eng = [Lang("eng".into())];
        assert_eq!(e.id(), EngineId("fake 1".into()));
        assert_eq!(
            e.recognize(Path::new("x"), &[], &go, later),
            Err(OcrError::NoLanguage)
        );
        assert_eq!(
            e.recognize(Path::new("x"), &eng, &AtomicBool::new(true), later),
            Err(OcrError::Cancelled)
        );
        assert_eq!(
            e.recognize(Path::new("x"), &eng, &go, Instant::now()),
            Err(OcrError::Timeout)
        );
        assert_eq!(e.recognize(Path::new("x"), &eng, &go, later), Ok(page));
    }
}
