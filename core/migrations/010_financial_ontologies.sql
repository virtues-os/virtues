-- Financial ontology tables for Plaid integration
-- These tables follow the existing ontology pattern with source_stream_id for deduplication

-- Financial accounts (linked bank/credit accounts)
CREATE TABLE IF NOT EXISTS data.financial_account (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- External identifiers
    account_id_external TEXT NOT NULL,              -- Provider's account ID (e.g., Plaid account_id)
    persistent_account_id TEXT,                     -- Plaid's persistent_account_id (stable across items)

    -- Account details
    account_name TEXT NOT NULL,
    official_name TEXT,                             -- Bank's official name for the account
    account_type TEXT NOT NULL,                     -- depository, credit, loan, investment, brokerage, other
    account_subtype TEXT,                           -- checking, savings, credit_card, mortgage, etc.
    mask TEXT,                                      -- Last 4 digits

    -- Balances (updated on each sync)
    current_balance NUMERIC(28,10),
    available_balance NUMERIC(28,10),
    credit_limit NUMERIC(28,10),
    currency_code TEXT DEFAULT 'USD',

    -- Institution info
    institution_id TEXT,
    institution_name TEXT,

    -- Status
    is_active BOOLEAN DEFAULT true,

    -- Standard ontology fields
    timestamp TIMESTAMPTZ NOT NULL,                 -- When this account was linked/last updated

    source_stream_id UUID NOT NULL,
    source_table TEXT NOT NULL DEFAULT 'stream_plaid_accounts',
    source_provider TEXT NOT NULL DEFAULT 'plaid',

    metadata JSONB DEFAULT '{}',

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT financial_account_unique_source UNIQUE (source_stream_id)
);

CREATE INDEX IF NOT EXISTS idx_financial_account_external_id
    ON data.financial_account(account_id_external);
CREATE INDEX IF NOT EXISTS idx_financial_account_type
    ON data.financial_account(account_type);
CREATE INDEX IF NOT EXISTS idx_financial_account_institution
    ON data.financial_account(institution_id);
CREATE INDEX IF NOT EXISTS idx_financial_account_timestamp
    ON data.financial_account(timestamp DESC);

DROP TRIGGER IF EXISTS financial_account_updated_at ON data.financial_account;
CREATE TRIGGER financial_account_updated_at
    BEFORE UPDATE ON data.financial_account
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at();


-- Financial transactions
CREATE TABLE IF NOT EXISTS data.financial_transaction (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- External identifiers
    transaction_id_external TEXT NOT NULL,          -- Provider's transaction ID (e.g., Plaid transaction_id)

    -- Account reference (resolved after account sync)
    account_id UUID REFERENCES data.financial_account(id),
    account_id_external TEXT NOT NULL,              -- For matching before account_id is resolved

    -- Amount (positive = money in, negative = money out)
    amount NUMERIC(28,10) NOT NULL,
    currency_code TEXT DEFAULT 'USD',

    -- Dates
    transaction_date DATE NOT NULL,                 -- Date transaction occurred
    authorized_date DATE,                           -- When authorized (may differ from transaction_date)
    posted_date DATE,                               -- When posted to account

    -- Description
    name TEXT NOT NULL,                             -- Original transaction description
    merchant_name TEXT,                             -- Cleaned merchant name (if available)

    -- Categorization
    category TEXT,                                  -- Normalized category (e.g., 'dining', 'groceries')
    category_detailed TEXT,                         -- Provider's detailed category
    personal_finance_category TEXT,                 -- Plaid's personal finance category

    -- Transaction type
    transaction_type TEXT,                          -- digital, place, special, unresolved
    payment_channel TEXT,                           -- online, in store, other

    -- Status
    is_pending BOOLEAN DEFAULT false,

    -- Location (optional, for in-store transactions)
    location_address TEXT,
    location_city TEXT,
    location_region TEXT,
    location_postal_code TEXT,
    location_country TEXT,
    location_lat FLOAT,
    location_lon FLOAT,

    -- Merchant info
    merchant_entity_id TEXT,                        -- Plaid merchant entity ID

    -- Standard ontology fields
    timestamp TIMESTAMPTZ NOT NULL,                 -- transaction_date as timestamp

    source_stream_id UUID NOT NULL,
    source_table TEXT NOT NULL DEFAULT 'stream_plaid_transactions',
    source_provider TEXT NOT NULL DEFAULT 'plaid',

    metadata JSONB DEFAULT '{}',

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT financial_transaction_unique_source UNIQUE (source_stream_id)
);

-- Primary indexes for queries
CREATE INDEX IF NOT EXISTS idx_financial_transaction_date
    ON data.financial_transaction(transaction_date DESC);
CREATE INDEX IF NOT EXISTS idx_financial_transaction_account_date
    ON data.financial_transaction(account_id, transaction_date DESC);
CREATE INDEX IF NOT EXISTS idx_financial_transaction_category
    ON data.financial_transaction(category);
CREATE INDEX IF NOT EXISTS idx_financial_transaction_merchant
    ON data.financial_transaction(merchant_name) WHERE merchant_name IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_financial_transaction_pending
    ON data.financial_transaction(is_pending) WHERE is_pending = true;
CREATE INDEX IF NOT EXISTS idx_financial_transaction_external_id
    ON data.financial_transaction(transaction_id_external);
CREATE INDEX IF NOT EXISTS idx_financial_transaction_account_external
    ON data.financial_transaction(account_id_external);
CREATE INDEX IF NOT EXISTS idx_financial_transaction_timestamp
    ON data.financial_transaction(timestamp DESC);

-- Full-text search on transaction names
CREATE INDEX IF NOT EXISTS idx_financial_transaction_search
    ON data.financial_transaction USING GIN(to_tsvector('english', coalesce(name, '') || ' ' || coalesce(merchant_name, '')));

DROP TRIGGER IF EXISTS financial_transaction_updated_at ON data.financial_transaction;
CREATE TRIGGER financial_transaction_updated_at
    BEFORE UPDATE ON data.financial_transaction
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at();
