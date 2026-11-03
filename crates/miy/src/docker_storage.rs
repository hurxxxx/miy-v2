//! Free-space floor on the Docker data root before work that adds images (V1-O1).
//!
//! On a single-host installation the platform database volume normally shares this
//! filesystem, so a server build or an image import must not consume its last free
//! space. The floor needs both enough bytes and enough percent free. An unknown
//! measurement refuses the work; setting both thresholds to 0 disables the check for
//! hosts whose Docker data root cannot be measured locally.
use crate::runtime_contract::{FailureCode, RuntimeFailure};
use anyhow::Result;

const GIB: u64 = 1 << 30;
pub(crate) const DEFAULT_MIN_FREE_GIB: u64 = 15;
pub(crate) const DEFAULT_MIN_FREE_PERCENT: u64 = 15;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Usage {
    pub(crate) total: u64,
    pub(crate) available: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Floor {
    min_free_bytes: u64,
    min_free_percent: u64,
}

impl Floor {
    pub(crate) fn from_env() -> Result<Self> {
        Self::new(
            read(
                "MIY_DOCKER_MIN_FREE_GIB",
                DEFAULT_MIN_FREE_GIB,
                u64::MAX / GIB,
            )?,
            read("MIY_DOCKER_MIN_FREE_PERCENT", DEFAULT_MIN_FREE_PERCENT, 100)?,
        )
    }

    fn new(gib: u64, percent: u64) -> Result<Self> {
        anyhow::ensure!(percent <= 100, "invalid_docker_storage_floor");
        Ok(Self {
            min_free_bytes: gib.saturating_mul(GIB),
            min_free_percent: percent,
        })
    }

    fn disabled(self) -> bool {
        self.min_free_bytes == 0 && self.min_free_percent == 0
    }

    fn allows(self, usage: Usage) -> bool {
        usage.total > 0
            && usage.available <= usage.total
            && usage.available >= self.min_free_bytes
            && u128::from(usage.available) * 100
                >= u128::from(usage.total) * u128::from(self.min_free_percent)
    }
}

fn read(name: &str, default: u64, max: u64) -> Result<u64> {
    match std::env::var(name) {
        Err(std::env::VarError::NotPresent) => Ok(default),
        Ok(value) => match value.trim().parse::<u64>() {
            Ok(parsed) if parsed <= max => Ok(parsed),
            _ => anyhow::bail!("invalid_docker_storage_floor"),
        },
        Err(_) => anyhow::bail!("invalid_docker_storage_floor"),
    }
}

/// Parses `df --output=size,avail -B1 PATH`: a header line, then bytes total and available.
fn parse_df(output: &str) -> Option<Usage> {
    let mut fields = output.lines().nth(1)?.split_whitespace();
    let total = fields.next()?.parse().ok()?;
    let available = fields.next()?.parse().ok()?;
    fields
        .next()
        .is_none()
        .then_some(Usage { total, available })
}

async fn measure() -> Result<Usage> {
    let unknown = || RuntimeFailure::new(FailureCode::DockerStorageUnknown);
    let root = crate::docker::docker(&["info", "--format", "{{.DockerRootDir}}"]).await?;
    if !root.starts_with('/') || root.contains('\n') {
        return Err(unknown().into());
    }
    let output = tokio::process::Command::new("df")
        .args(["--output=size,avail", "-B1", root.as_str()])
        .stdin(std::process::Stdio::null())
        .output()
        .await
        .map_err(|_| unknown())?;
    if !output.status.success() {
        return Err(unknown().into());
    }
    parse_df(&String::from_utf8_lossy(&output.stdout)).ok_or_else(|| unknown().into())
}

/// Refuses work that adds images when the Docker data root is below the floor.
pub(crate) async fn preflight() -> Result<()> {
    check(Floor::from_env()?).await
}

async fn check(floor: Floor) -> Result<()> {
    if floor.disabled() {
        return Ok(());
    }
    let usage = measure().await?;
    if floor.allows(usage) {
        return Ok(());
    }
    tracing::warn!(
        event = "docker_storage_low",
        available_bytes = usage.available,
        total_bytes = usage.total
    );
    Err(RuntimeFailure::new(FailureCode::DockerStorageLow).into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usage(total_gib: u64, available_gib: u64) -> Usage {
        Usage {
            total: total_gib * GIB,
            available: available_gib * GIB,
        }
    }

    #[test]
    fn floor_needs_both_bytes_and_percent() {
        let floor = Floor::new(15, 15).unwrap();
        assert!(floor.allows(usage(100, 15)));
        assert!(!floor.allows(usage(100, 14)), "15% but below 15 GiB");
        assert!(floor.allows(usage(937, 227)));
        assert!(!floor.allows(usage(937, 140)), "15 GiB free but below 15%");
        assert!(!floor.allows(Usage {
            total: 0,
            available: 0
        }));
        assert!(!floor.allows(Usage {
            total: 1,
            available: 2
        }));
        assert!(!floor.disabled());
        assert!(Floor::new(0, 0).unwrap().disabled());
        assert!(Floor::new(0, 101).is_err());
    }

    #[test]
    fn df_output_is_parsed_strictly() {
        let sample = "     1B-blocks       Avail\n1006045179904 243745320960\n";
        assert_eq!(
            parse_df(sample),
            Some(Usage {
                total: 1_006_045_179_904,
                available: 243_745_320_960
            })
        );
        assert_eq!(parse_df("header only\n"), None);
        assert_eq!(parse_df("h\n12 x\n"), None);
        assert_eq!(parse_df("h\n12 3 4\n"), None);
    }

    #[tokio::test]
    #[ignore = "needs a local Docker daemon"]
    async fn the_real_docker_root_is_measured_and_a_full_floor_refuses() {
        let usage = measure().await.unwrap();
        assert!(usage.total > 0 && usage.available <= usage.total);
        assert!(check(Floor::new(0, 0).unwrap()).await.is_ok());
        let refused = check(Floor::new(0, 100).unwrap()).await.unwrap_err();
        assert_eq!(
            crate::runtime_contract::classify_failure(&refused).code,
            FailureCode::DockerStorageLow
        );
    }
}
