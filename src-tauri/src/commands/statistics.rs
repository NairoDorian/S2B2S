use crate::managers::statistics::{StatisticsManager, StatisticsRange, StatisticsSummary};
use std::sync::Arc;
use tauri::State;

#[tauri::command]
#[specta::specta]
pub async fn get_statistics_summary(
    statistics_manager: State<'_, Arc<StatisticsManager>>,
    range: StatisticsRange,
) -> Result<StatisticsSummary, String> {
    range.validate().map_err(|error| error.to_string())?;
    // SQLite I/O: keep it off the webview thread.
    let manager = Arc::clone(statistics_manager.inner());
    tauri::async_runtime::spawn_blocking(move || manager.summarize(range))
        .await
        .map_err(|error| format!("Statistics task failed: {error}"))?
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn reset_statistics(
    statistics_manager: State<'_, Arc<StatisticsManager>>,
) -> Result<(), String> {
    let manager = Arc::clone(statistics_manager.inner());
    tauri::async_runtime::spawn_blocking(move || manager.reset())
        .await
        .map_err(|error| format!("Statistics task failed: {error}"))?
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_ranges_before_querying() {
        assert!(
            StatisticsRange {
                start_ms: 10.0,
                end_ms: 10.0,
            }
            .validate()
            .is_err()
        );
        assert!(
            StatisticsRange {
                start_ms: -1.0,
                end_ms: 10.0,
            }
            .validate()
            .is_err()
        );
    }
}
