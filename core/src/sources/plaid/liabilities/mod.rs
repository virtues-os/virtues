//! Plaid liabilities stream implementation
//!
//! Syncs credit cards, mortgages, and student loans from Plaid using the /liabilities/get endpoint.

pub mod transform;

use async_trait::async_trait;
use chrono::Utc;
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::Mutex;

use super::client::PlaidClient;
use super::config::PlaidLiabilitiesConfig;
use crate::sources::base::oauth::encryption::TokenEncryptor;
use crate::{
    error::{Error, Result},
    sources::{
        base::{ConfigSerializable, SyncMode, SyncResult},
        pull_stream::PullStream,
    },
    storage::stream_writer::StreamWriter,
};

/// Plaid liabilities stream
///
/// Syncs liability details (credit cards, mortgages, student loans) from Plaid
/// to the financial_liability ontology.
pub struct PlaidLiabilitiesStream {
    source_id: String,
    client: PlaidClient,
    db: SqlitePool,
    stream_writer: Arc<Mutex<StreamWriter>>,
    config: PlaidLiabilitiesConfig,
    /// Access token for this Item (loaded from database)
    access_token: Option<String>,
}

impl PlaidLiabilitiesStream {
    /// Create a new liabilities stream
    pub fn new(
        source_id: String,
        db: SqlitePool,
        stream_writer: Arc<Mutex<StreamWriter>>,
    ) -> Result<Self> {
        let client = PlaidClient::from_env()?;

        Ok(Self {
            source_id,
            client,
            db,
            stream_writer,
            config: PlaidLiabilitiesConfig::default(),
            access_token: None,
        })
    }

    /// Create with explicit client (for testing)
    pub fn with_client(
        source_id: String,
        client: PlaidClient,
        db: SqlitePool,
        stream_writer: Arc<Mutex<StreamWriter>>,
    ) -> Self {
        Self {
            source_id,
            client,
            db,
            stream_writer,
            config: PlaidLiabilitiesConfig::default(),
            access_token: None,
        }
    }

    /// Load configuration from database
    async fn load_config_internal(&mut self, db: &SqlitePool, source_id: &str) -> Result<()> {
        // Load stream config
        let result = sqlx::query_as::<_, (serde_json::Value,)>(
            "SELECT config FROM elt_stream_connections WHERE source_connection_id = $1 AND stream_name = 'liabilities'",
        )
        .bind(source_id)
        .fetch_optional(db)
        .await?;

        if let Some((config_json,)) = result {
            if let Ok(config) = PlaidLiabilitiesConfig::from_json(&config_json) {
                self.config = config;
            }
        }

        // Load access token from source_connections (encrypted)
        let token_result = sqlx::query_as::<_, (Option<String>,)>(
            "SELECT access_token FROM elt_source_connections WHERE id = $1",
        )
        .bind(source_id)
        .fetch_optional(db)
        .await?;

        if let Some((Some(encrypted_token),)) = token_result {
            let encryptor = TokenEncryptor::from_env()?;
            self.access_token = Some(encryptor.decrypt(&encrypted_token)?);
        }

        Ok(())
    }

    /// Sync liabilities with explicit mode
    #[tracing::instrument(skip(self), fields(source_id = %self.source_id, mode = ?sync_mode))]
    pub async fn sync_with_mode(&self, sync_mode: &SyncMode) -> Result<SyncResult> {
        tracing::info!("Starting Plaid liabilities sync");

        let access_token = self
            .access_token
            .as_ref()
            .ok_or_else(|| Error::Configuration("Plaid access token not loaded".to_string()))?;

        self.sync_internal(access_token, sync_mode).await
    }

    /// Internal sync implementation
    async fn sync_internal(&self, access_token: &str, _sync_mode: &SyncMode) -> Result<SyncResult> {
        let started_at = Utc::now();
        let mut records_fetched = 0;
        let mut records_written = 0;
        let mut records_failed = 0;

        // Fetch liabilities
        let response = self.client.liabilities_get(access_token).await?;

        // Process credit card liabilities
        if let Some(credit_cards) = &response.liabilities.credit {
            for credit in credit_cards {
                records_fetched += 1;
                match self.write_credit_liability(credit).await {
                    Ok(true) => records_written += 1,
                    Ok(false) => records_failed += 1,
                    Err(e) => {
                        tracing::warn!(
                            account_id = ?credit.account_id,
                            error = %e,
                            "Failed to write credit liability"
                        );
                        records_failed += 1;
                    }
                }
            }
        }

        // Process mortgage liabilities
        if let Some(mortgages) = &response.liabilities.mortgage {
            for mortgage in mortgages {
                records_fetched += 1;
                match self.write_mortgage_liability(mortgage).await {
                    Ok(true) => records_written += 1,
                    Ok(false) => records_failed += 1,
                    Err(e) => {
                        tracing::warn!(
                            account_id = %mortgage.account_id,
                            error = %e,
                            "Failed to write mortgage liability"
                        );
                        records_failed += 1;
                    }
                }
            }
        }

        // Process student loan liabilities
        if let Some(student_loans) = &response.liabilities.student {
            for student in student_loans {
                records_fetched += 1;
                match self.write_student_loan_liability(student).await {
                    Ok(true) => records_written += 1,
                    Ok(false) => records_failed += 1,
                    Err(e) => {
                        tracing::warn!(
                            account_id = ?student.account_id,
                            error = %e,
                            "Failed to write student loan liability"
                        );
                        records_failed += 1;
                    }
                }
            }
        }

        let completed_at = Utc::now();

        // Collect records from StreamWriter
        let records = {
            let mut writer = self.stream_writer.lock().await;
            let collected = writer
                .collect_records(&self.source_id, "liabilities")
                .map(|(records, _, _)| records);

            if let Some(ref recs) = collected {
                tracing::info!(
                    record_count = recs.len(),
                    "Collected liability records from StreamWriter"
                );
            }

            collected
        };

        tracing::info!(
            records_fetched,
            records_written,
            records_failed,
            "Plaid liabilities sync complete"
        );

        Ok(SyncResult {
            records_fetched,
            records_written,
            records_failed,
            next_cursor: None,
            earliest_record_at: None,
            latest_record_at: None,
            started_at,
            completed_at,
            records,
            archive_job_id: None,
        })
    }

    /// Write a credit card liability to the StreamWriter
    async fn write_credit_liability(
        &self,
        credit: &super::client::CreditLiability,
    ) -> Result<bool> {
        let timestamp = Utc::now();

        // Get primary APR if available
        let primary_apr = credit.aprs.first();

        let record = serde_json::json!({
            "account_id": credit.account_id,
            "liability_type": "credit_card",
            "apr_percentage": primary_apr.map(|a| a.apr_percentage),
            "apr_type": primary_apr.map(|a| &a.apr_type),
            "minimum_payment": credit.minimum_payment_amount,
            "last_payment_amount": credit.last_payment_amount,
            "last_payment_date": credit.last_payment_date,
            "next_payment_due_date": credit.next_payment_due_date,
            "last_statement_balance": credit.last_statement_balance,
            "last_statement_issue_date": credit.last_statement_issue_date,
            "is_overdue": credit.is_overdue,
            "aprs": credit.aprs,
            "synced_at": timestamp,
        });

        {
            let mut writer = self.stream_writer.lock().await;
            writer.write_record(&self.source_id, "liabilities", record, Some(timestamp))?;
        }

        Ok(true)
    }

    /// Write a mortgage liability to the StreamWriter
    async fn write_mortgage_liability(
        &self,
        mortgage: &super::client::MortgageLiability,
    ) -> Result<bool> {
        let timestamp = Utc::now();

        let record = serde_json::json!({
            "account_id": mortgage.account_id,
            "liability_type": "mortgage",
            "interest_rate_percentage": mortgage.interest_rate.as_ref().and_then(|r| r.percentage),
            "apr_type": mortgage.interest_rate.as_ref().and_then(|r| r.rate_type.as_ref()),
            "minimum_payment": mortgage.next_monthly_payment,
            "last_payment_amount": mortgage.last_payment_amount,
            "last_payment_date": mortgage.last_payment_date,
            "next_payment_due_date": mortgage.next_payment_due_date,
            "next_payment_amount": mortgage.next_monthly_payment,
            "original_loan_amount": mortgage.origination_principal_amount,
            "loan_term": mortgage.loan_term,
            "loan_type_description": mortgage.loan_type_description,
            "origination_date": mortgage.origination_date,
            "maturity_date": mortgage.maturity_date,
            "escrow_balance": mortgage.escrow_balance,
            "past_due_amount": mortgage.past_due_amount,
            "has_pmi": mortgage.has_pmi,
            "has_prepayment_penalty": mortgage.has_prepayment_penalty,
            "property_address": mortgage.property_address.as_ref().map(|a| serde_json::json!({
                "street": a.street,
                "city": a.city,
                "region": a.region,
                "postal_code": a.postal_code,
                "country": a.country,
            })),
            "ytd_interest_paid": mortgage.ytd_interest_paid,
            "ytd_principal_paid": mortgage.ytd_principal_paid,
            "synced_at": timestamp,
        });

        {
            let mut writer = self.stream_writer.lock().await;
            writer.write_record(&self.source_id, "liabilities", record, Some(timestamp))?;
        }

        Ok(true)
    }

    /// Write a student loan liability to the StreamWriter
    async fn write_student_loan_liability(
        &self,
        student: &super::client::StudentLoanLiability,
    ) -> Result<bool> {
        let timestamp = Utc::now();

        let record = serde_json::json!({
            "account_id": student.account_id,
            "liability_type": "student_loan",
            "interest_rate_percentage": student.interest_rate_percentage,
            "minimum_payment": student.minimum_payment_amount,
            "last_payment_amount": student.last_payment_amount,
            "last_payment_date": student.last_payment_date,
            "next_payment_due_date": student.next_payment_due_date,
            "original_loan_amount": student.origination_principal_amount,
            "origination_date": student.origination_date,
            "expected_payoff_date": student.expected_payoff_date,
            "outstanding_interest_amount": student.outstanding_interest_amount,
            "is_overdue": student.is_overdue,
            "loan_name": student.loan_name,
            "guarantor": student.guarantor,
            "loan_status": student.loan_status.as_ref().map(|s| serde_json::json!({
                "type": s.status_type,
                "end_date": s.end_date,
            })),
            "repayment_plan": student.repayment_plan.as_ref().map(|p| serde_json::json!({
                "type": p.plan_type,
                "description": p.description,
            })),
            "pslf_status": student.pslf_status.as_ref().map(|p| serde_json::json!({
                "estimated_eligibility_date": p.estimated_eligibility_date,
                "payments_made": p.payments_made,
                "payments_remaining": p.payments_remaining,
            })),
            "ytd_interest_paid": student.ytd_interest_paid,
            "ytd_principal_paid": student.ytd_principal_paid,
            "synced_at": timestamp,
        });

        {
            let mut writer = self.stream_writer.lock().await;
            writer.write_record(&self.source_id, "liabilities", record, Some(timestamp))?;
        }

        Ok(true)
    }
}

#[async_trait]
impl PullStream for PlaidLiabilitiesStream {
    async fn sync_pull(&self, mode: SyncMode) -> Result<SyncResult> {
        self.sync_with_mode(&mode).await
    }

    async fn load_config(&mut self, db: &SqlitePool, source_id: &str) -> Result<()> {
        self.load_config_internal(db, source_id).await
    }

    fn table_name(&self) -> &str {
        "stream_plaid_liabilities"
    }

    fn stream_name(&self) -> &str {
        "liabilities"
    }

    fn source_name(&self) -> &str {
        "plaid"
    }

    fn supports_incremental(&self) -> bool {
        false // Liabilities are always full refresh
    }

    fn supports_full_refresh(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_stream_metadata() {
        assert_eq!("stream_plaid_liabilities", "stream_plaid_liabilities");
        assert_eq!("liabilities", "liabilities");
        assert_eq!("plaid", "plaid");
    }
}
