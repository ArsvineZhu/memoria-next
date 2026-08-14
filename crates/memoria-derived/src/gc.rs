use std::collections::BTreeSet;

use crate::lease::unix_now;
use crate::{ArtifactId, DerivedCatalog, DerivedError, ManifestId};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct GcReport {
    pub deleted_manifests: Vec<ManifestId>,
    pub collected_artifacts: Vec<ArtifactId>,
}

pub struct DerivedGc<'a> {
    catalog: &'a mut DerivedCatalog,
    grace: BTreeSet<ManifestId>,
    pins: BTreeSet<ManifestId>,
}

impl<'a> DerivedGc<'a> {
    pub(crate) const fn new(catalog: &'a mut DerivedCatalog) -> Self {
        Self {
            catalog,
            grace: BTreeSet::new(),
            pins: BTreeSet::new(),
        }
    }

    pub fn retain_grace_manifest(&mut self, manifest_id: ManifestId) -> Result<(), DerivedError> {
        self.catalog.manifest(manifest_id)?;
        self.grace.insert(manifest_id);
        Ok(())
    }

    pub fn pin_manifest(&mut self, manifest_id: ManifestId) -> Result<(), DerivedError> {
        self.catalog.manifest(manifest_id)?;
        self.pins.insert(manifest_id);
        Ok(())
    }

    pub fn collect(self) -> Result<GcReport, DerivedError> {
        let mut roots = self.grace;
        roots.extend(self.pins);
        let transaction = self.catalog.begin_immediate_with_retry()?;
        let now = unix_now();
        transaction.execute("DELETE FROM leases WHERE expires_at <= ?1", [now])?;

        let current = transaction
            .query_row(
                "SELECT manifest_id FROM serving_pointer WHERE singleton = 1",
                [],
                |row| row.get::<_, Option<i64>>(0),
            )?
            .map(ManifestId::from_raw);
        if let Some(current) = current {
            roots.insert(current);
        }
        let mut statement = transaction
            .prepare("SELECT manifest_id FROM leases WHERE expires_at > ?1 ORDER BY manifest_id")?;
        let leases = statement
            .query_map([now], |row| row.get::<_, i64>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        drop(statement);
        roots.extend(leases.into_iter().map(ManifestId::from_raw));

        let mut statement = transaction.prepare("SELECT id FROM manifests ORDER BY id")?;
        let manifests = statement
            .query_map([], |row| row.get::<_, i64>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        drop(statement);
        let mut deleted_manifests = Vec::new();
        let mut removed_artifacts = BTreeSet::new();
        for raw_id in manifests {
            let manifest_id = ManifestId::from_raw(raw_id);
            if roots.contains(&manifest_id) {
                continue;
            }
            let mut statement = transaction
                .prepare("SELECT artifact_id FROM manifest_artifacts WHERE manifest_id = ?1")?;
            let artifacts = statement
                .query_map([raw_id], |row| row.get::<_, i64>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            drop(statement);
            removed_artifacts.extend(artifacts.into_iter().map(ArtifactId::from_raw));
            transaction.execute(
                "DELETE FROM manifest_artifacts WHERE manifest_id = ?1",
                [raw_id],
            )?;
            transaction.execute(
                "DELETE FROM manifest_capabilities WHERE manifest_id = ?1",
                [raw_id],
            )?;
            transaction.execute("DELETE FROM leases WHERE manifest_id = ?1", [raw_id])?;
            transaction.execute("DELETE FROM manifests WHERE id = ?1", [raw_id])?;
            deleted_manifests.push(manifest_id);
        }

        let mut collected_artifacts = Vec::new();
        for artifact_id in removed_artifacts {
            let still_referenced = transaction.query_row(
                "SELECT EXISTS (
                    SELECT 1 FROM manifest_artifacts WHERE artifact_id = ?1
                )",
                [artifact_id.value()],
                |row| row.get::<_, bool>(0),
            )?;
            if still_referenced {
                continue;
            }
            let active_build = transaction.query_row(
                "SELECT EXISTS (
                    SELECT 1 FROM build_jobs
                    WHERE state IN ('queued', 'running')
                )",
                [],
                |row| row.get::<_, bool>(0),
            )?;
            if active_build {
                continue;
            }
            transaction.execute(
                "DELETE FROM ann_tombstones WHERE artifact_id = ?1",
                [artifact_id.value()],
            )?;
            transaction.execute(
                "DELETE FROM ann_segments WHERE artifact_id = ?1",
                [artifact_id.value()],
            )?;
            transaction.execute(
                "DELETE FROM vector_memberships WHERE artifact_id = ?1",
                [artifact_id.value()],
            )?;
            transaction.execute(
                "UPDATE artifacts SET state = 'collected' WHERE id = ?1
                 AND state NOT IN ('planned', 'building', 'staged', 'validated')",
                [artifact_id.value()],
            )?;
            collected_artifacts.push(artifact_id);
        }
        transaction.commit()?;
        Ok(GcReport {
            deleted_manifests,
            collected_artifacts,
        })
    }
}
