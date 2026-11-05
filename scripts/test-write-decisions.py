#!/usr/bin/env python3
"""Tests for check-write-decisions.py on synthetic sources."""
import importlib.util
import pathlib
import tempfile
import unittest

SCRIPT = pathlib.Path(__file__).resolve().parent / "check-write-decisions.py"
spec = importlib.util.spec_from_file_location("check_write_decisions", SCRIPT)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

HANDLERS = '''
pub async fn decided(s: &State, p: &Principal) -> ApiResult<()> {
    let mut tx = policy::Write::begin(&s.pool).await?;
    sqlx::query("INSERT INTO things VALUES($1)").execute(&mut *tx).await?;
    tx.decide(p, "app", "dev", AppAction::Develop).await?.commit().await
}
pub async fn raw(s: &State, p: &Principal) -> ApiResult<()> {
    let mut tx = s.pool.begin().await?;
    sqlx::query("DELETE FROM things").execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}
async fn pool_write(
    S(s): S<State>,
    axum::Extension(p): P,
) -> ApiResult<()> {
    sqlx::query("UPDATE things SET name=$1").bind(&p.id).execute(&s.pool).await?;
    Ok(())
}
async fn worker(pool: &PgPool) -> Result<()> {
    sqlx::query("UPDATE things SET seen=true").execute(pool).await?;
    Ok(())
}
#[cfg(test)]
mod tests {
    async fn fixture(pool: &PgPool, p: &Principal) {
        let tx = pool.begin().await.unwrap();
        sqlx::query("INSERT INTO things VALUES(1)");
    }
}
'''
REGRESSION = '''
async fn setup(pool: &PgPool, p: &Principal) {
    let tx = pool.begin().await.unwrap();
}
'''


class WriteDecisionTests(unittest.TestCase):
    def run_check(self, transactions, writers):
        module.TRANSACTIONS, module.WRITERS = transactions, writers
        with tempfile.TemporaryDirectory() as root:
            (pathlib.Path(root) / "handlers.rs").write_text(HANDLERS)
            (pathlib.Path(root) / "things_regression.rs").write_text(REGRESSION)
            return module.check(root)

    def test_raw_transactions_and_principal_pool_writes_are_reported(self):
        self.assertEqual(
            self.run_check({}, {}),
            [
                "handlers.rs: raw opens a transaction outside policy::Write",
                "handlers.rs: raw writes for a principal outside policy::Write",
                "handlers.rs: pool_write writes for a principal outside policy::Write",
            ],
        )

    def test_listed_functions_pass_and_unused_entries_fail(self):
        self.assertEqual(
            self.run_check(
                {("handlers.rs", "raw"): "own"},
                {("handlers.rs", "raw"): "own", ("handlers.rs", "pool_write"): "own"},
            ),
            [],
        )
        self.assertEqual(
            self.run_check(
                {("handlers.rs", "raw"): "own", ("handlers.rs", "gone"): "worker"},
                {("handlers.rs", "raw"): "own", ("handlers.rs", "pool_write"): "own"},
            ),
            ["handlers.rs: gone is listed for a raw transaction it no longer opens"],
        )

    def test_the_repository_passes(self):
        # A fresh copy keeps the script's own lists; the other tests replace them.
        fresh = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(fresh)
        self.assertEqual(fresh.check(SCRIPT.parent.parent / "crates/miy/src"), [])


if __name__ == "__main__":
    unittest.main()
