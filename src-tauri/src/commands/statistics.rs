use crate::managers::statistics::{StatisticsManager, StatisticsRange, StatisticsSummary};
use std::sync::Arc;
use tauri::State;

#[tauri::command]
#[specta::specta]
pub fn get_statistics_summary(
    statistics_manager: State<'_, Arc<StatisticsManager>>,
    range: StatisticsRange,
) -> Result<StatisticsSummary, String> {
    range.validate().map_err(|error| error.to_string())?;
    statistics_manager
        .summarize(range)
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn reset_statistics(
    statistics_manager: State<'_, Arc<StatisticsManager>>,
) -> Result<usize, String> {
    statistics_manager
        .reset()
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_ranges_before_querying() {
        assert!(
            StatisticsRange {
                start_ms: 10,
                end_ms: 10,
            }
            .validate()
            .is_err()
        );
        assert!(
            StatisticsRange {
                start_ms: -1,
                end_ms: 10,
            }
            .validate()
            .is_err()
        );
    }
}
