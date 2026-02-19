use std::collections::{HashMap, HashSet};
use std::io::Cursor;

use axum::{
    extract::Multipart,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use calamine::{open_workbook_auto_from_rs, Data, Reader};
use serde::Serialize;
use serde_json::Value;
use zip::ZipArchive;

const ORG_ID: &str = "cd861b76-f85c-4afc-b3e8-8f85945c3132";

#[derive(Debug, Clone, Default)]
struct ParsedData {
    files: HashMap<String, String>,
    customers: Vec<CustomerRow>,
    suppliers: Vec<SupplierRow>,
    employees: Vec<EmployeeRow>,
    journal: Vec<JournalEntry>,
    general_ledger: Vec<GLRow>,
    trial_balance: Vec<TrialBalanceRow>,
    profit_and_loss: Vec<PLRow>,
    balance_sheet: Vec<BSRow>,
    parse_errors: Vec<String>,
}

#[derive(Debug, Clone)]
struct CustomerRow {
    name: String,
    phone: Option<String>,
    email: Option<String>,
    full_name: Option<String>,
    billing_address: Option<String>,
}

#[derive(Debug, Clone)]
struct SupplierRow {
    name: String,
    phone: Option<String>,
    email: Option<String>,
    full_name: Option<String>,
    address: Option<String>,
}

#[derive(Debug, Clone)]
struct EmployeeRow {
    name: String,
    phone: Option<String>,
    email: Option<String>,
}

#[derive(Debug, Clone)]
struct JournalLine {
    memo: Option<String>,
    account: String,
    debit: f64,
    credit: f64,
}

#[derive(Debug, Clone)]
struct JournalEntry {
    date: String,
    kind: String,
    num: Option<String>,
    name: Option<String>,
    lines: Vec<JournalLine>,
}

#[derive(Debug, Clone)]
struct GLRow {
    account: String,
}

#[derive(Debug, Clone)]
struct TrialBalanceRow {
    debit: f64,
    credit: f64,
}

#[derive(Debug, Clone)]
struct PLRow {}

#[derive(Debug, Clone)]
struct BSRow {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DataTypeKey {
    Customers,
    Suppliers,
    Employees,
    Journal,
    GeneralLedger,
    TrialBalance,
    ProfitAndLoss,
    BalanceSheet,
}

impl DataTypeKey {
    fn as_str(self) -> &'static str {
        match self {
            DataTypeKey::Customers => "customers",
            DataTypeKey::Suppliers => "suppliers",
            DataTypeKey::Employees => "employees",
            DataTypeKey::Journal => "journal",
            DataTypeKey::GeneralLedger => "generalLedger",
            DataTypeKey::TrialBalance => "trialBalance",
            DataTypeKey::ProfitAndLoss => "profitAndLoss",
            DataTypeKey::BalanceSheet => "balanceSheet",
        }
    }
}

#[derive(Debug, Serialize, Default)]
struct ImportReport {
    accounts: AccountsReport,
    customers: EntityReport,
    suppliers: EntityReport,
    employees: EmployeeReport,
    #[serde(rename = "journalEntries")]
    journal_entries: JournalReport,
    #[serde(rename = "bankAccounts")]
    bank_accounts: BankAccountsReport,
    #[serde(rename = "generalLedger")]
    general_ledger: RowsNote,
    #[serde(rename = "trialBalance")]
    trial_balance: RowsNote,
    #[serde(rename = "profitAndLoss")]
    profit_and_loss: RowsNote,
    #[serde(rename = "balanceSheet")]
    balance_sheet: RowsNote,
    warnings: Vec<String>,
    #[serde(rename = "dryRunNotice")]
    dry_run_notice: Option<String>,
    debug: DebugReport,
}

#[derive(Debug, Serialize, Default)]
struct AccountsReport {
    created: usize,
    reused: usize,
    unmapped: Vec<String>,
}

#[derive(Debug, Serialize, Default)]
struct EntityReport {
    imported: usize,
    skipped: usize,
    details: Vec<String>,
}

#[derive(Debug, Serialize, Default)]
struct EmployeeReport {
    parsed: usize,
    note: String,
}

#[derive(Debug, Serialize, Default)]
struct JournalReport {
    imported: usize,
    errors: usize,
    #[serde(rename = "totalLines")]
    total_lines: usize,
    #[serde(rename = "unmappedAccounts")]
    unmapped_accounts: Vec<String>,
}

#[derive(Debug, Serialize, Default)]
struct BankAccountsReport {
    created: usize,
    details: Vec<String>,
}

#[derive(Debug, Serialize, Default)]
struct RowsNote {
    rows: usize,
    note: String,
}

#[derive(Debug, Serialize, Default)]
struct DebugReport {
    #[serde(rename = "filesDetected")]
    files_detected: usize,
    #[serde(rename = "parseErrors")]
    parse_errors: usize,
    customers: usize,
    suppliers: usize,
    employees: usize,
    #[serde(rename = "journalEntries")]
    journal_entries: usize,
    #[serde(rename = "journalLines")]
    journal_lines: usize,
    #[serde(rename = "generalLedger")]
    general_ledger: usize,
    #[serde(rename = "trialBalance")]
    trial_balance: usize,
    #[serde(rename = "profitAndLoss")]
    profit_and_loss: usize,
    #[serde(rename = "balanceSheet")]
    balance_sheet: usize,
}

#[derive(Debug, Serialize)]
struct ImportResponse {
    success: bool,
    #[serde(rename = "dryRun")]
    dry_run: bool,
    #[serde(rename = "dryRunNotice")]
    dry_run_notice: Option<String>,
    purge: bool,
    #[serde(rename = "purgeReport")]
    purge_report: Option<HashMap<String, i64>>,
    report: ImportReport,
    #[serde(rename = "filesFound")]
    files_found: Vec<String>,
    timestamp: String,
    error: Option<String>,
}

pub async fn greenbooks_import_quickbooks(mut multipart: Multipart) -> Response {
    let mut file_data: Option<Vec<u8>> = None;
    let mut dry_run = true;
    let mut purge = false;

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or_default().to_string();
        if name == "file" {
            match field.bytes().await {
                Ok(bytes) => file_data = Some(bytes.to_vec()),
                Err(e) => {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(serde_json::json!({"success": false, "error": format!("invalid file: {e}")})),
                    )
                        .into_response()
                }
            }
        } else {
            let value = field.text().await.unwrap_or_default().to_ascii_lowercase();
            if name == "dryRun" {
                dry_run = value == "true";
            } else if name == "purge" {
                purge = value == "true";
            }
        }
    }

    let Some(file_data) = file_data else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"success": false, "error": "missing multipart file field"})),
        )
            .into_response();
    };

    let parsed = parse_qb_blob(&file_data);
    let files_found: Vec<String> = parsed.files.keys().cloned().collect();
    let mut report = base_report(&parsed, dry_run);

    let mut purge_report = None;
    if purge {
        match run_purge(dry_run).await {
            Ok(r) => purge_report = Some(r),
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ImportResponse {
                        success: false,
                        dry_run,
                        dry_run_notice: report.dry_run_notice.clone(),
                        purge,
                        purge_report: None,
                        report,
                        files_found,
                        timestamp: chrono::Utc::now().to_rfc3339(),
                        error: Some(format!("purge failed: {e}")),
                    }),
                )
                    .into_response()
            }
        }
    }

    let import_result = run_import(&parsed, dry_run, &mut report).await;
    if let Err(e) = import_result {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ImportResponse {
                success: false,
                dry_run,
                dry_run_notice: report.dry_run_notice.clone(),
                purge,
                purge_report,
                report,
                files_found,
                timestamp: chrono::Utc::now().to_rfc3339(),
                error: Some(e),
            }),
        )
            .into_response();
    }

    (
        StatusCode::OK,
        Json(ImportResponse {
            success: true,
            dry_run,
            dry_run_notice: report.dry_run_notice.clone(),
            purge,
            purge_report,
            report,
            files_found,
            timestamp: chrono::Utc::now().to_rfc3339(),
            error: None,
        }),
    )
        .into_response()
}

fn base_report(parsed: &ParsedData, dry_run: bool) -> ImportReport {
    let journal_lines = parsed.journal.iter().map(|e| e.lines.len()).sum();
    ImportReport {
        employees: EmployeeReport {
            parsed: parsed.employees.len(),
            note: "Employees parsed for reference (no gb_employees table)".to_string(),
        },
        general_ledger: RowsNote {
            rows: parsed.general_ledger.len(),
            note: "GL data used for cross-validation; journal entries are the primary import"
                .to_string(),
        },
        trial_balance: RowsNote {
            rows: parsed.trial_balance.len(),
            note: "Trial balance used for validation".to_string(),
        },
        profit_and_loss: RowsNote {
            rows: parsed.profit_and_loss.len(),
            note: "P&L used for validation".to_string(),
        },
        balance_sheet: RowsNote {
            rows: parsed.balance_sheet.len(),
            note: "Balance sheet used for validation".to_string(),
        },
        warnings: parsed.parse_errors.clone(),
        dry_run_notice: dry_run
            .then_some("Dry run mode: no data was written to the database.".to_string()),
        debug: DebugReport {
            files_detected: parsed.files.len(),
            parse_errors: parsed.parse_errors.len(),
            customers: parsed.customers.len(),
            suppliers: parsed.suppliers.len(),
            employees: parsed.employees.len(),
            journal_entries: parsed.journal.len(),
            journal_lines,
            general_ledger: parsed.general_ledger.len(),
            trial_balance: parsed.trial_balance.len(),
            profit_and_loss: parsed.profit_and_loss.len(),
            balance_sheet: parsed.balance_sheet.len(),
        },
        ..Default::default()
    }
}

async fn run_import(
    parsed: &ParsedData,
    dry_run: bool,
    report: &mut ImportReport,
) -> Result<(), String> {
    let (base, key) = supabase_env()?;
    let client = supabase_client_with_key(&key)?;

    let account_map = setup_accounts(&client, &base, dry_run, report).await?;
    import_customers(&client, &base, parsed, dry_run, report).await?;
    import_suppliers(&client, &base, parsed, dry_run, report).await?;
    import_bank_accounts(&client, &base, &account_map, dry_run, report).await?;
    import_journal(&client, &base, parsed, &account_map, dry_run, report).await?;

    if !parsed.journal.is_empty() {
        report.warnings.push(
            "GST/HST mapping: QuickBooks Journal export does not include tax codes/rates, so tax cannot be inferred. Use a Tax Detail report or map tax accounts manually after import.".to_string()
        );
    }

    validate_trial_balance(parsed, report);
    Ok(())
}

async fn run_purge(dry_run: bool) -> Result<HashMap<String, i64>, String> {
    let (base, key) = supabase_env()?;
    let client = supabase_client_with_key(&key)?;
    let purge_order = [
        "gb_ledger_entries",
        "gb_bank_transactions",
        "gb_bank_reconciliations",
        "gb_reconciliations",
        "gb_expense_items",
        "gb_expenses",
        "gb_bill_payments",
        "gb_bill_items",
        "gb_bills",
        "gb_invoice_items",
        "gb_payments",
        "gb_invoices",
        "gb_journal_entry_lines",
        "gb_journal_entries",
        "gb_vendors",
        "gb_customers",
        "gb_bank_accounts",
        "gb_audit_log",
        "gb_fiscal_periods",
        "gb_recurring_templates",
    ];

    let mut out = HashMap::new();
    for table in purge_order {
        let count_url = format!("{base}/rest/v1/{table}?select=*&org_id=eq.{ORG_ID}&limit=1",);
        let resp = client
            .get(&count_url)
            .header("Prefer", "count=exact")
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            out.insert(table.to_string(), -1);
            continue;
        }
        let total = resp
            .headers()
            .get("content-range")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.rsplit('/').next())
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);

        if dry_run || total <= 0 {
            out.insert(table.to_string(), total.max(0));
            continue;
        }

        let del_url = format!("{base}/rest/v1/{table}?org_id=eq.{ORG_ID}");
        let del = client
            .delete(&del_url)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if del.status().is_success() {
            out.insert(table.to_string(), total.max(0));
        } else {
            out.insert(table.to_string(), -1);
        }
    }

    Ok(out)
}

async fn setup_accounts(
    client: &reqwest::Client,
    base: &str,
    dry_run: bool,
    report: &mut ImportReport,
) -> Result<HashMap<String, String>, String> {
    let mut account_map = HashMap::new();

    for m in QB_ACCOUNT_MAP {
        if m.existing {
            if let Some(id) = find_account_id(client, base, m.code).await? {
                account_map.insert(m.qb_name.to_string(), id);
                report.accounts.reused += 1;
            } else {
                report.warnings.push(format!(
                    "Expected existing account {} ({}) not found",
                    m.code, m.name
                ));
            }
            continue;
        }

        if let Some(id) = find_account_id(client, base, m.code).await? {
            account_map.insert(m.qb_name.to_string(), id);
            report.accounts.reused += 1;
            continue;
        }

        if dry_run {
            account_map.insert(m.qb_name.to_string(), format!("dry-run-{}", m.code));
            report.accounts.created += 1;
            continue;
        }

        let payload = serde_json::json!([{
            "org_id": ORG_ID,
            "code": m.code,
            "name": m.name,
            "account_type": m.account_type,
            "sub_type": m.sub_type,
            "normal_balance": m.normal_balance,
            "is_active": true,
            "is_system": false
        }]);
        let url = format!("{base}/rest/v1/gb_accounts?on_conflict=org_id,code");
        let rows = post_rows_with_prefer(
            client,
            &url,
            &payload,
            "resolution=merge-duplicates,return=representation",
        )
        .await?;
        if let Some(id) = rows
            .first()
            .and_then(|r| r.get("id"))
            .and_then(|v| v.as_str())
            .map(str::to_string)
        {
            account_map.insert(m.qb_name.to_string(), id);
            report.accounts.created += 1;
        }
    }

    Ok(account_map)
}

async fn import_customers(
    client: &reqwest::Client,
    base: &str,
    parsed: &ParsedData,
    dry_run: bool,
    report: &mut ImportReport,
) -> Result<(), String> {
    for c in &parsed.customers {
        if c.name.trim().is_empty() {
            report.customers.skipped += 1;
            continue;
        }
        if dry_run {
            report.customers.imported += 1;
            report
                .customers
                .details
                .push(format!("[DRY-RUN] Would import customer: {}", c.name));
            continue;
        }

        if exists_by_name(client, base, "gb_customers", &c.name).await? {
            report.customers.skipped += 1;
            report
                .customers
                .details
                .push(format!("Skipped (exists): {}", c.name));
            continue;
        }

        let (a1, a2, city, province, postal, country) = parse_address(c.billing_address.as_deref());
        let payload = serde_json::json!([{
            "org_id": ORG_ID,
            "name": c.name,
            "email": c.email,
            "phone": c.phone,
            "company": c.full_name.as_ref().filter(|v| *v != &c.name).cloned(),
            "address_line1": a1,
            "address_line2": a2,
            "city": city,
            "province": province,
            "postal_code": postal,
            "country": country
        }]);
        match post_rows(client, &format!("{base}/rest/v1/gb_customers"), &payload).await {
            Ok(_) => {
                report.customers.imported += 1;
                report
                    .customers
                    .details
                    .push(format!("Imported: {}", c.name));
            }
            Err(e) => {
                report.customers.skipped += 1;
                report
                    .warnings
                    .push(format!("Error importing customer {}: {e}", c.name));
            }
        }
    }
    Ok(())
}

async fn import_suppliers(
    client: &reqwest::Client,
    base: &str,
    parsed: &ParsedData,
    dry_run: bool,
    report: &mut ImportReport,
) -> Result<(), String> {
    for s in &parsed.suppliers {
        if s.name.trim().is_empty() {
            report.suppliers.skipped += 1;
            continue;
        }
        if dry_run {
            report.suppliers.imported += 1;
            report
                .suppliers
                .details
                .push(format!("[DRY-RUN] Would import supplier: {}", s.name));
            continue;
        }

        if exists_by_name(client, base, "gb_vendors", &s.name).await? {
            report.suppliers.skipped += 1;
            report
                .suppliers
                .details
                .push(format!("Skipped (exists): {}", s.name));
            continue;
        }

        let (a1, a2, city, province, postal, country) = parse_address(s.address.as_deref());
        let payload = serde_json::json!([{
            "org_id": ORG_ID,
            "name": s.name,
            "email": s.email,
            "phone": s.phone,
            "company": s.full_name.as_ref().filter(|v| *v != &s.name).cloned(),
            "address_line1": a1,
            "address_line2": a2,
            "city": city,
            "province": province,
            "postal_code": postal,
            "country": country
        }]);
        match post_rows(client, &format!("{base}/rest/v1/gb_vendors"), &payload).await {
            Ok(_) => {
                report.suppliers.imported += 1;
                report
                    .suppliers
                    .details
                    .push(format!("Imported: {}", s.name));
            }
            Err(e) => {
                report.suppliers.skipped += 1;
                report
                    .warnings
                    .push(format!("Error importing vendor {}: {e}", s.name));
            }
        }
    }
    Ok(())
}

async fn import_bank_accounts(
    client: &reqwest::Client,
    base: &str,
    account_map: &HashMap<String, String>,
    dry_run: bool,
    report: &mut ImportReport,
) -> Result<(), String> {
    for b in BANK_ACCOUNTS {
        let gl_account_id = account_map.get(b.qb_account);

        if dry_run {
            report.bank_accounts.created += 1;
            report
                .bank_accounts
                .details
                .push(format!("[DRY-RUN] Would create bank account: {}", b.name));
            continue;
        }

        let Some(gl_account_id) = gl_account_id else {
            report
                .warnings
                .push(format!("No GL account mapped for bank: {}", b.name));
            continue;
        };
        if gl_account_id.starts_with("dry-run-") {
            report.warnings.push(format!(
                "No persisted GL account mapped for bank: {}",
                b.name
            ));
            continue;
        }

        if exists_by_name(client, base, "gb_bank_accounts", b.name).await? {
            report
                .bank_accounts
                .details
                .push(format!("Skipped (exists): {}", b.name));
            continue;
        }

        let payload = serde_json::json!([{
            "org_id": ORG_ID,
            "name": b.name,
            "institution": b.institution,
            "account_number_last4": b.last4,
            "gl_account_id": gl_account_id,
            "currency": b.currency,
            "is_active": true
        }]);

        match post_rows(
            client,
            &format!("{base}/rest/v1/gb_bank_accounts"),
            &payload,
        )
        .await
        {
            Ok(_) => {
                report.bank_accounts.created += 1;
                report
                    .bank_accounts
                    .details
                    .push(format!("Created: {}", b.name));
            }
            Err(e) => report
                .warnings
                .push(format!("Error creating bank account {}: {e}", b.name)),
        }
    }

    Ok(())
}

async fn import_journal(
    client: &reqwest::Client,
    base: &str,
    parsed: &ParsedData,
    account_map: &HashMap<String, String>,
    dry_run: bool,
    report: &mut ImportReport,
) -> Result<(), String> {
    let mut unmapped = HashSet::new();

    for (idx, entry) in parsed.journal.iter().enumerate() {
        let mut lines_payload = Vec::new();
        let mut missing_map = false;

        for (line_idx, line) in entry.lines.iter().enumerate() {
            let Some(account_id) = account_map.get(&line.account) else {
                unmapped.insert(line.account.clone());
                missing_map = true;
                continue;
            };

            lines_payload.push(serde_json::json!({
                "account_id": account_id,
                "description": line.memo,
                "debit": line.debit,
                "credit": line.credit,
                "sort_order": line_idx
            }));
        }

        if missing_map {
            report.journal_entries.errors += 1;
            continue;
        }
        if lines_payload.len() < 2 {
            report.journal_entries.errors += 1;
            report.warnings.push(format!(
                "JE {} {}: less than 2 lines, skipped",
                entry.date, entry.kind
            ));
            continue;
        }

        let total_debit: f64 = entry.lines.iter().map(|l| l.debit).sum();
        let total_credit: f64 = entry.lines.iter().map(|l| l.credit).sum();
        if (total_debit - total_credit).abs() > 0.01 {
            report.journal_entries.errors += 1;
            report.warnings.push(format!(
                "JE {} is unbalanced: debit={:.2}, credit={:.2}",
                entry.date, total_debit, total_credit
            ));
            continue;
        }

        report.journal_entries.total_lines += lines_payload.len();
        if dry_run {
            report.journal_entries.imported += 1;
            continue;
        }

        let year = entry.date.split('-').next().unwrap_or("2025");
        let entry_num = format!("QBI-{year}-{:04}", idx + 1);
        let desc = [
            Some(entry.kind.clone()),
            entry.name.clone(),
            entry.lines.first().and_then(|l| l.memo.clone()),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" - ");

        let je_payload = serde_json::json!([{
            "org_id": ORG_ID,
            "entry_number": entry_num,
            "entry_date": entry.date,
            "description": desc,
            "reference": entry.num.as_ref().map(|n| format!("QB#{n}")),
            "source": format!("qb_import_{}", sanitize_source(&entry.kind)),
            "status": "posted",
            "posted_at": chrono::Utc::now().to_rfc3339(),
            "total_debit": round2(total_debit),
            "total_credit": round2(total_credit)
        }]);

        let je_rows = match post_rows(
            client,
            &format!("{base}/rest/v1/gb_journal_entries"),
            &je_payload,
        )
        .await
        {
            Ok(v) => v,
            Err(e) => {
                report.journal_entries.errors += 1;
                report.warnings.push(format!("Error JE {}: {e}", idx + 1));
                continue;
            }
        };

        let Some(je_id) = je_rows
            .first()
            .and_then(|r| r.get("id"))
            .and_then(|v| v.as_str())
            .map(str::to_string)
        else {
            report.journal_entries.errors += 1;
            continue;
        };

        let line_rows: Vec<Value> = lines_payload
            .into_iter()
            .map(|mut l| {
                l["journal_entry_id"] = Value::String(je_id.clone());
                l
            })
            .collect();

        let inserted_lines = match post_rows(
            client,
            &format!("{base}/rest/v1/gb_journal_entry_lines"),
            &Value::Array(line_rows),
        )
        .await
        {
            Ok(v) => v,
            Err(e) => {
                report.journal_entries.errors += 1;
                report
                    .warnings
                    .push(format!("Error JE lines {}: {e}", idx + 1));
                continue;
            }
        };

        let ledger_rows: Vec<Value> = inserted_lines
            .into_iter()
            .filter_map(|r| {
                let id = r.get("id")?.as_str()?.to_string();
                let account_id = r.get("account_id")?.as_str()?.to_string();
                let debit = r.get("debit").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let credit = r.get("credit").and_then(|v| v.as_f64()).unwrap_or(0.0);
                Some(serde_json::json!({
                    "org_id": ORG_ID,
                    "account_id": account_id,
                    "journal_entry_id": je_id,
                    "journal_line_id": id,
                    "entry_date": entry.date,
                    "debit": debit,
                    "credit": credit,
                }))
            })
            .collect();
        if !ledger_rows.is_empty() {
            let _ = post_rows(
                client,
                &format!("{base}/rest/v1/gb_ledger_entries"),
                &Value::Array(ledger_rows),
            )
            .await
            .map_err(|e| {
                report
                    .warnings
                    .push(format!("Error ledger for JE {}: {e}", idx + 1));
                e
            });
        }

        report.journal_entries.imported += 1;
    }

    report.journal_entries.unmapped_accounts = unmapped.clone().into_iter().collect();
    report.accounts.unmapped = unmapped.into_iter().collect();
    Ok(())
}

fn validate_trial_balance(parsed: &ParsedData, report: &mut ImportReport) {
    if parsed.trial_balance.is_empty() {
        return;
    }

    let tb_total_debit: f64 = parsed.trial_balance.iter().map(|r| r.debit).sum();
    let tb_total_credit: f64 = parsed.trial_balance.iter().map(|r| r.credit).sum();

    let je_total_debit: f64 = parsed
        .journal
        .iter()
        .flat_map(|e| e.lines.iter())
        .map(|l| l.debit)
        .sum();
    let je_total_credit: f64 = parsed
        .journal
        .iter()
        .flat_map(|e| e.lines.iter())
        .map(|l| l.credit)
        .sum();

    if (tb_total_debit - tb_total_credit).abs() > 0.01 {
        report.warnings.push(format!(
            "QB Trial Balance is unbalanced: Debit={:.2}, Credit={:.2}",
            tb_total_debit, tb_total_credit
        ));
    }

    report.warnings.push(format!(
        "Validation: QB TB totals D={:.2} C={:.2} | JE totals D={:.2} C={:.2}",
        tb_total_debit, tb_total_credit, je_total_debit, je_total_credit
    ));
}

fn parse_qb_blob(data: &[u8]) -> ParsedData {
    let mut parsed = ParsedData::default();
    let is_zip = data.len() >= 4 && data[0] == 0x50 && data[1] == 0x4b;

    if is_zip {
        if let Ok(mut zip) = ZipArchive::new(Cursor::new(data)) {
            let mut excel_count = 0;
            for i in 0..zip.len() {
                let Ok(mut file) = zip.by_index(i) else {
                    continue;
                };
                let name = file.name().to_string();
                if name.ends_with('/')
                    || name.starts_with("__MACOSX")
                    || name.contains("/.")
                    || !(name.ends_with(".xlsx")
                        || name.ends_with(".xls")
                        || name.ends_with(".csv"))
                {
                    continue;
                }
                excel_count += 1;
                let basename = name.rsplit('/').next().unwrap_or(&name).to_string();
                let mut buf = vec![];
                if std::io::Read::read_to_end(&mut file, &mut buf).is_ok() {
                    parse_workbook_bytes(&basename, &buf, &mut parsed);
                }
            }
            if excel_count == 0 {
                parsed
                    .parse_errors
                    .push("No Excel files found in ZIP archive.".to_string());
            }
        } else {
            parsed.parse_errors.push("Invalid ZIP archive.".to_string());
        }
    } else {
        parse_workbook_bytes("upload.xlsx", data, &mut parsed);
    }

    parsed
}

fn parse_workbook_bytes(name: &str, data: &[u8], parsed: &mut ParsedData) {
    let mut wb = match open_workbook_auto_from_rs(Cursor::new(data.to_vec())) {
        Ok(w) => w,
        Err(e) => {
            parsed
                .parse_errors
                .push(format!("Error parsing {name}: {e}"));
            return;
        }
    };

    let sheet_names = wb.sheet_names().to_vec();
    if sheet_names.is_empty() {
        parsed.parse_errors.push(format!("no sheet in {name}"));
        return;
    }

    let mut detected_any = false;
    for sheet_name in sheet_names {
        let Ok(range) = wb.worksheet_range(&sheet_name) else {
            continue;
        };

        let rows: Vec<Vec<String>> = range
            .rows()
            .map(|r| r.iter().map(data_to_string).collect::<Vec<String>>())
            .collect();

        let detected = detect_type(name, &sheet_name, &rows);
        if let Some(t) = detected {
            detected_any = true;
            parsed.files.insert(
                if sheet_name == "Sheet1" {
                    name.to_string()
                } else {
                    format!("{name}:{sheet_name}")
                },
                t.as_str().to_string(),
            );
            parse_rows_by_type(&rows, t, parsed);
        }
    }

    if !detected_any {
        parsed.files.insert(name.to_string(), "unknown".to_string());
        parsed
            .parse_errors
            .push(format!("Could not detect type for: {name}"));
    }
}

fn detect_type(name: &str, sheet_name: &str, rows: &[Vec<String>]) -> Option<DataTypeKey> {
    let lower_name = name.to_ascii_lowercase();
    let lower_sheet = sheet_name.to_ascii_lowercase();
    let header_text = rows
        .iter()
        .take(10)
        .flat_map(|r| r.iter())
        .filter(|s| !s.trim().is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();

    let probe = [
        lower_name.as_str(),
        lower_sheet.as_str(),
        header_text.as_str(),
    ]
    .join(" ");

    if probe.contains("customer") {
        return Some(DataTypeKey::Customers);
    }
    if probe.contains("supplier") || probe.contains("vendor") {
        return Some(DataTypeKey::Suppliers);
    }
    if probe.contains("employee") {
        return Some(DataTypeKey::Employees);
    }
    if probe.contains("journal")
        || (probe.contains("debit") && probe.contains("credit") && probe.contains("memo"))
    {
        return Some(DataTypeKey::Journal);
    }
    if probe.contains("general ledger") || probe.contains("general_ledger") || probe.contains(" gl")
    {
        return Some(DataTypeKey::GeneralLedger);
    }
    if probe.contains("trial balance") {
        return Some(DataTypeKey::TrialBalance);
    }
    if (probe.contains("profit") && probe.contains("loss")) || probe.contains("income statement") {
        return Some(DataTypeKey::ProfitAndLoss);
    }
    if probe.contains("balance sheet") {
        return Some(DataTypeKey::BalanceSheet);
    }

    None
}

fn parse_rows_by_type(rows: &[Vec<String>], ty: DataTypeKey, parsed: &mut ParsedData) {
    match ty {
        DataTypeKey::Customers => parsed.customers.extend(parse_customers(rows)),
        DataTypeKey::Suppliers => parsed.suppliers.extend(parse_suppliers(rows)),
        DataTypeKey::Employees => parsed.employees.extend(parse_employees(rows)),
        DataTypeKey::Journal => parsed.journal.extend(parse_journal(rows)),
        DataTypeKey::GeneralLedger => parsed.general_ledger.extend(parse_general_ledger(rows)),
        DataTypeKey::TrialBalance => parsed.trial_balance.extend(parse_trial_balance(rows)),
        DataTypeKey::ProfitAndLoss => parsed.profit_and_loss.extend(parse_profit_and_loss(rows)),
        DataTypeKey::BalanceSheet => parsed.balance_sheet.extend(parse_balance_sheet(rows)),
    }
}

fn find_header_row(rows: &[Vec<String>]) -> usize {
    for (i, row) in rows.iter().take(20).enumerate() {
        let text = row
            .iter()
            .filter(|s| !s.trim().is_empty())
            .cloned()
            .collect::<Vec<_>>()
            .join(" ")
            .to_ascii_lowercase();
        let has_date = text.contains("date");
        let has_debit_credit = text.contains("debit") || text.contains("credit");
        let has_accountish =
            text.contains("account") || text.contains("memo") || text.contains("description");
        let has_balance = text.contains("balance");

        if text.contains("phone") || text.contains("email") {
            return i;
        }
        if has_date && (has_debit_credit || has_balance) && has_accountish {
            return i;
        }
    }
    4
}

fn detect_offset(row: &[String]) -> usize {
    if row.first().map(|v| !v.trim().is_empty()).unwrap_or(false) {
        0
    } else {
        1
    }
}

fn is_end_of_data(v: &str) -> bool {
    let s = v.trim();
    s.starts_with("Total")
        || s.starts_with("TOTAL")
        || s.starts_with("Monday,")
        || s.starts_with("Tuesday,")
        || s.starts_with("Wednesday,")
        || s.starts_with("Thursday,")
        || s.starts_with("Friday,")
        || s.starts_with("Saturday,")
        || s.starts_with("Sunday,")
}

fn parse_customers(rows: &[Vec<String>]) -> Vec<CustomerRow> {
    let mut out = Vec::new();
    let header_idx = find_header_row(rows);
    for row in rows.iter().skip(header_idx + 1) {
        let offset = detect_offset(row);
        let name = row
            .get(offset)
            .cloned()
            .unwrap_or_default()
            .trim()
            .to_string();
        if name.is_empty() {
            continue;
        }
        if is_end_of_data(&name) {
            break;
        }

        out.push(CustomerRow {
            name,
            phone: clean_phone(row.get(offset + 1)),
            email: extract_email(row, offset),
            full_name: str_opt(row.get(offset + 3).map(String::as_str)),
            billing_address: str_opt(row.get(offset + 4).map(String::as_str)),
        });
    }
    out
}

fn parse_suppliers(rows: &[Vec<String>]) -> Vec<SupplierRow> {
    let mut out = Vec::new();
    let header_idx = find_header_row(rows);
    for row in rows.iter().skip(header_idx + 1) {
        let offset = detect_offset(row);
        let name = row
            .get(offset)
            .cloned()
            .unwrap_or_default()
            .trim()
            .to_string();
        if name.is_empty() {
            continue;
        }
        if is_end_of_data(&name) {
            break;
        }

        out.push(SupplierRow {
            name,
            phone: clean_phone(row.get(offset + 1)),
            email: extract_email(row, offset),
            full_name: str_opt(row.get(offset + 3).map(String::as_str)),
            address: str_opt(row.get(offset + 4).map(String::as_str)),
        });
    }
    out
}

fn parse_employees(rows: &[Vec<String>]) -> Vec<EmployeeRow> {
    let mut out = Vec::new();
    let header_idx = find_header_row(rows);
    for row in rows.iter().skip(header_idx + 1) {
        let offset = detect_offset(row);
        let name = row
            .get(offset)
            .cloned()
            .unwrap_or_default()
            .trim()
            .to_string();
        if name.is_empty() {
            continue;
        }
        if is_end_of_data(&name) {
            break;
        }

        out.push(EmployeeRow {
            name,
            phone: clean_phone(row.get(offset + 1)),
            email: extract_email(row, offset),
        });
    }
    out
}

#[derive(Default)]
struct JournalCols {
    date: Option<usize>,
    tx_type: Option<usize>,
    num: Option<usize>,
    name: Option<usize>,
    memo: Option<usize>,
    account: Option<usize>,
    debit: Option<usize>,
    credit: Option<usize>,
}

fn parse_journal(rows: &[Vec<String>]) -> Vec<JournalEntry> {
    let mut entries = Vec::new();
    let mut current: Option<JournalEntry> = None;

    let header_idx = find_header_row(rows);
    let cols = map_journal_columns(rows.get(header_idx).cloned().unwrap_or_default());

    for row in rows.iter().skip(header_idx + 1) {
        if row.iter().all(|c| c.trim().is_empty()) {
            continue;
        }

        let offset = detect_offset(row);
        if is_journal_account_header_row(row, &cols, offset) {
            continue;
        }

        let date = get_journal_cell(row, cols.date, offset, 0);
        let account = get_journal_cell(row, cols.account, offset, 5);
        let debit = parse_num(get_journal_cell(row, cols.debit, offset, 6));
        let credit = parse_num(get_journal_cell(row, cols.credit, offset, 7));
        let memo = str_opt(get_journal_cell(row, cols.memo, offset, 4));

        if is_date_like(date) {
            if let Some(prev) = current.take() {
                if !prev.lines.is_empty() {
                    entries.push(prev);
                }
            }
            let mut next = JournalEntry {
                date: normalize_date(date),
                kind: str_opt(get_journal_cell(row, cols.tx_type, offset, 1))
                    .unwrap_or_else(|| "Journal".to_string()),
                num: str_opt(get_journal_cell(row, cols.num, offset, 2)),
                name: str_opt(get_journal_cell(row, cols.name, offset, 3)),
                lines: vec![],
            };
            if let Some(account) = str_opt(account) {
                next.lines.push(JournalLine {
                    memo,
                    account,
                    debit,
                    credit,
                });
            }
            current = Some(next);
            continue;
        }

        if let Some(c) = current.as_mut() {
            if let Some(account) = str_opt(account) {
                c.lines.push(JournalLine {
                    memo,
                    account,
                    debit,
                    credit,
                });
            }
        }
    }

    if let Some(last) = current {
        if !last.lines.is_empty() {
            entries.push(last);
        }
    }

    entries
}

fn parse_general_ledger(rows: &[Vec<String>]) -> Vec<GLRow> {
    let mut out = Vec::new();
    let header_idx = find_header_row(rows);
    let mut current_account = String::new();

    for row in rows.iter().skip(header_idx + 1) {
        let first = row
            .first()
            .or_else(|| row.get(1))
            .map(String::as_str)
            .unwrap_or_default()
            .trim();
        if first.is_empty() {
            continue;
        }

        if !is_date_like(Some(first)) && !first.starts_with("Total") && !first.starts_with("TOTAL")
        {
            let col2 = row.get(2).map(String::as_str).unwrap_or_default();
            let col3 = row.get(3).map(String::as_str).unwrap_or_default();
            let col4 = row.get(4).map(String::as_str).unwrap_or_default();
            if col2.trim().is_empty()
                && parse_num(Some(col3)) == 0.0
                && parse_num(Some(col4)) == 0.0
            {
                current_account = first.to_string();
                continue;
            }
        }

        let offset = detect_offset(row);
        let date = row.get(offset).map(String::as_str);
        if is_date_like(date) {
            out.push(GLRow {
                account: current_account.clone(),
            });
        }
    }

    out
}

fn parse_trial_balance(rows: &[Vec<String>]) -> Vec<TrialBalanceRow> {
    let mut out = Vec::new();
    let header_idx = find_header_row(rows);
    for row in rows.iter().skip(header_idx + 1) {
        let offset = detect_offset(row);
        let account = row
            .get(offset)
            .map(String::as_str)
            .unwrap_or_default()
            .trim();
        if account.is_empty()
            || account.starts_with("Total")
            || account.starts_with("TOTAL")
            || is_end_of_data(account)
        {
            continue;
        }
        let debit = parse_num(row.get(offset + 1).map(String::as_str));
        let credit = parse_num(row.get(offset + 2).map(String::as_str));
        if debit != 0.0 || credit != 0.0 {
            out.push(TrialBalanceRow { debit, credit });
        }
    }
    out
}

fn parse_profit_and_loss(rows: &[Vec<String>]) -> Vec<PLRow> {
    let mut out = Vec::new();
    for row in rows {
        let label = row
            .first()
            .or_else(|| row.get(1))
            .map(String::as_str)
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        if label.is_empty()
            || label.starts_with("total")
            || label == "income"
            || label == "expenses"
            || label == "revenue"
        {
            continue;
        }
        let offset = if row.first().map(|x| !x.trim().is_empty()).unwrap_or(false) {
            0
        } else {
            1
        };
        let amount = parse_num(row.get(offset + 1).map(String::as_str));
        if amount != 0.0 {
            out.push(PLRow {});
        }
    }
    out
}

fn parse_balance_sheet(rows: &[Vec<String>]) -> Vec<BSRow> {
    let mut out = Vec::new();
    for row in rows {
        let label = row
            .first()
            .or_else(|| row.get(1))
            .map(String::as_str)
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        if label.is_empty()
            || label.starts_with("total")
            || label == "assets"
            || label == "liabilities"
            || label == "equity"
        {
            continue;
        }
        let offset = if row.first().map(|x| !x.trim().is_empty()).unwrap_or(false) {
            0
        } else {
            1
        };
        let amount = parse_num(row.get(offset + 1).map(String::as_str));
        if amount != 0.0 {
            out.push(BSRow {});
        }
    }
    out
}

fn map_journal_columns(row: Vec<String>) -> JournalCols {
    let mut cols = JournalCols::default();
    for (idx, cell) in row.iter().enumerate() {
        let label = cell
            .to_ascii_lowercase()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if label.contains("date") {
            cols.date = Some(idx);
        } else if label.contains("type") {
            cols.tx_type = Some(idx);
        } else if label.contains("num") || label.contains("no.") {
            cols.num = Some(idx);
        } else if label.contains("name") || label.contains("payee") {
            cols.name = Some(idx);
        } else if label.contains("memo") || label.contains("description") {
            cols.memo = Some(idx);
        } else if label.contains("account") {
            cols.account = Some(idx);
        } else if label.contains("debit") {
            cols.debit = Some(idx);
        } else if label.contains("credit") {
            cols.credit = Some(idx);
        }
    }
    cols
}

fn get_journal_cell<'a>(
    row: &'a [String],
    col: Option<usize>,
    offset: usize,
    fallback: usize,
) -> Option<&'a str> {
    if let Some(c) = col {
        return row.get(c).map(String::as_str);
    }
    row.get(offset + fallback).map(String::as_str)
}

fn is_journal_account_header_row(row: &[String], cols: &JournalCols, offset: usize) -> bool {
    let date = get_journal_cell(row, cols.date, offset, 0);
    if is_date_like(date) {
        return false;
    }

    let account = get_journal_cell(row, cols.account, offset, 5)
        .unwrap_or_default()
        .trim();
    if account.is_empty() {
        return false;
    }

    let debit = parse_num(get_journal_cell(row, cols.debit, offset, 6));
    let credit = parse_num(get_journal_cell(row, cols.credit, offset, 7));
    if debit != 0.0 || credit != 0.0 {
        return false;
    }

    let type_s = get_journal_cell(row, cols.tx_type, offset, 1)
        .unwrap_or_default()
        .trim();
    let num_s = get_journal_cell(row, cols.num, offset, 2)
        .unwrap_or_default()
        .trim();
    let name_s = get_journal_cell(row, cols.name, offset, 3)
        .unwrap_or_default()
        .trim();
    let memo_s = get_journal_cell(row, cols.memo, offset, 4)
        .unwrap_or_default()
        .trim();
    if !(type_s.is_empty() && num_s.is_empty() && name_s.is_empty() && memo_s.is_empty()) {
        return false;
    }

    true
}

fn data_to_string(d: &Data) -> String {
    match d {
        Data::String(s) => s.clone(),
        Data::Float(f) => {
            if f.fract() == 0.0 {
                (*f as i64).to_string()
            } else {
                f.to_string()
            }
        }
        Data::Int(i) => i.to_string(),
        Data::Bool(b) => b.to_string(),
        Data::DateTime(dt) => dt.to_string(),
        _ => String::new(),
    }
}

fn parse_num(s: Option<&str>) -> f64 {
    let Some(s) = s else {
        return 0.0;
    };
    if s.trim().is_empty() {
        return 0.0;
    }
    let neg_wrapped = s.trim().starts_with('(') && s.trim().ends_with(')');
    let cleaned = s.replace(['$', ',', ' '], "").replace(['(', ')'], "");
    let parsed = cleaned.parse::<f64>().unwrap_or(0.0);
    if neg_wrapped {
        -parsed
    } else {
        parsed
    }
}

fn is_date_like(v: Option<&str>) -> bool {
    let Some(v) = v else {
        return false;
    };
    let s = v.trim();
    let slash = s.split('/').collect::<Vec<_>>();
    if slash.len() == 3 && slash.iter().all(|p| p.chars().all(|c| c.is_ascii_digit())) {
        return true;
    }
    s.len() >= 10
        && s.chars().nth(4) == Some('-')
        && s.chars().nth(7) == Some('-')
        && s.chars().take(10).all(|c| c.is_ascii_digit() || c == '-')
}

fn normalize_date(v: Option<&str>) -> String {
    let s = v.unwrap_or_default().trim();
    let slash = s.split('/').collect::<Vec<_>>();
    if slash.len() == 3 {
        let y = if slash[2].len() == 2 {
            format!("20{}", slash[2])
        } else {
            slash[2].to_string()
        };
        return format!("{}-{:0>2}-{:0>2}", y, slash[0], slash[1]);
    }
    if s.len() >= 10 {
        s[..10].to_string()
    } else {
        s.to_string()
    }
}

fn clean_phone(v: Option<&String>) -> Option<String> {
    let Some(v) = v else {
        return None;
    };
    let mut s = v.trim().to_string();
    for p in ["Phone:", "Mobile:", "Home:", "Work:", "Fax:"] {
        if s.to_ascii_lowercase().starts_with(&p.to_ascii_lowercase()) {
            s = s[p.len()..].trim().to_string();
            break;
        }
    }
    (!s.is_empty()).then_some(s)
}

fn extract_email(row: &[String], offset: usize) -> Option<String> {
    let end = (offset + 5).min(row.len());
    for v in row.iter().take(end).skip(offset + 1) {
        let s = v.trim();
        if s.contains('@') {
            return Some(s.to_string());
        }
    }
    None
}

fn str_opt(v: Option<&str>) -> Option<String> {
    let s = v.unwrap_or_default().trim();
    (!s.is_empty()).then_some(s.to_string())
}

fn parse_address(
    addr: Option<&str>,
) -> (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
) {
    let Some(addr) = addr else {
        return (None, None, None, None, None, None);
    };
    let parts: Vec<String> = addr
        .split('\n')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if parts.is_empty() {
        return (None, None, None, None, None, None);
    }

    let line1 = parts.first().cloned();
    let mut line2 = None;
    let mut city = None;
    let mut province = None;
    let mut postal = None;
    let mut country = None;

    if parts.len() >= 3 {
        let city_line = parts.get(parts.len() - 2).cloned().unwrap_or_default();
        let tokens = city_line.split_whitespace().collect::<Vec<_>>();
        if tokens.len() >= 3 {
            province = Some(tokens[tokens.len() - 2].to_string());
            postal = Some(tokens[tokens.len() - 1].to_string());
            city = Some(tokens[..tokens.len() - 2].join(" "));
        }

        let country_line = parts
            .last()
            .cloned()
            .unwrap_or_default()
            .to_ascii_lowercase();
        country = if country_line.contains("can") || country_line.contains("canada") {
            Some("CA".to_string())
        } else {
            Some("US".to_string())
        };

        if parts.len() > 3 {
            line2 = Some(parts[1..parts.len() - 2].join(", "));
        }
    }

    (line1, line2, city, province, postal, country)
}

fn sanitize_source(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_ascii_alphabetic() {
            out.push(c.to_ascii_lowercase());
        } else {
            out.push('_');
        }
    }
    out
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

fn supabase_env() -> Result<(String, String), String> {
    let base = std::env::var("SUPABASE_URL")
        .or_else(|_| std::env::var("NEXT_PUBLIC_SUPABASE_URL"))
        .unwrap_or_default()
        .trim_end_matches('/')
        .to_string();
    let key = std::env::var("SUPABASE_SERVICE_ROLE_KEY").unwrap_or_default();
    if base.is_empty() || key.is_empty() {
        return Err("Supabase env not configured in gateway".to_string());
    }
    Ok((base, key))
}

fn supabase_client_with_key(key: &str) -> Result<reqwest::Client, String> {
    let mut headers = reqwest::header::HeaderMap::new();
    let key_val = reqwest::header::HeaderValue::from_str(key).map_err(|e| e.to_string())?;
    headers.insert("apikey", key_val.clone());
    headers.insert(
        reqwest::header::AUTHORIZATION,
        reqwest::header::HeaderValue::from_str(&format!("Bearer {key}"))
            .map_err(|e| e.to_string())?,
    );
    reqwest::Client::builder()
        .default_headers(headers)
        .build()
        .map_err(|e| e.to_string())
}

async fn find_account_id(
    client: &reqwest::Client,
    base: &str,
    code: &str,
) -> Result<Option<String>, String> {
    let url =
        format!("{base}/rest/v1/gb_accounts?select=id&org_id=eq.{ORG_ID}&code=eq.{code}&limit=1");
    let resp = client.get(url).send().await.map_err(|e| e.to_string())?;
    let status = resp.status();
    let txt = resp.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(txt);
    }
    let rows: Vec<Value> = serde_json::from_str(&txt).unwrap_or_default();
    Ok(rows
        .first()
        .and_then(|r| r.get("id"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string()))
}

async fn exists_by_name(
    client: &reqwest::Client,
    base: &str,
    table: &str,
    name: &str,
) -> Result<bool, String> {
    let escaped = urlencoding::encode(name);
    let url =
        format!("{base}/rest/v1/{table}?select=id&org_id=eq.{ORG_ID}&name=eq.{escaped}&limit=1");
    let resp = client.get(url).send().await.map_err(|e| e.to_string())?;
    let status = resp.status();
    let txt = resp.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(txt);
    }
    let rows: Vec<Value> = serde_json::from_str(&txt).unwrap_or_default();
    Ok(!rows.is_empty())
}

async fn post_rows(
    client: &reqwest::Client,
    url: &str,
    payload: &Value,
) -> Result<Vec<Value>, String> {
    post_rows_with_prefer(client, url, payload, "return=representation").await
}

async fn post_rows_with_prefer(
    client: &reqwest::Client,
    url: &str,
    payload: &Value,
    prefer: &str,
) -> Result<Vec<Value>, String> {
    let resp = client
        .post(url)
        .header("Prefer", prefer)
        .json(payload)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = resp.status();
    let txt = resp.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(txt);
    }
    Ok(serde_json::from_str::<Vec<Value>>(&txt).unwrap_or_default())
}

struct AccountMapDef {
    qb_name: &'static str,
    code: &'static str,
    name: &'static str,
    account_type: &'static str,
    sub_type: &'static str,
    normal_balance: &'static str,
    existing: bool,
}

const QB_ACCOUNT_MAP: &[AccountMapDef] = &[
    AccountMapDef {
        qb_name: "RBC CAD 7336",
        code: "1011",
        name: "RBC CAD 7336",
        account_type: "asset",
        sub_type: "bank",
        normal_balance: "debit",
        existing: false,
    },
    AccountMapDef {
        qb_name: "RBC USD",
        code: "1012",
        name: "RBC USD",
        account_type: "asset",
        sub_type: "bank",
        normal_balance: "debit",
        existing: false,
    },
    AccountMapDef {
        qb_name: "Venn - 2756",
        code: "1013",
        name: "Venn - 2756",
        account_type: "asset",
        sub_type: "bank",
        normal_balance: "debit",
        existing: false,
    },
    AccountMapDef {
        qb_name: "Venn - 5516",
        code: "1014",
        name: "Venn - 5516",
        account_type: "asset",
        sub_type: "bank",
        normal_balance: "debit",
        existing: false,
    },
    AccountMapDef {
        qb_name: "Venn:0265 SaaS",
        code: "2101",
        name: "Venn CC 0265 SaaS",
        account_type: "liability",
        sub_type: "current_liability",
        normal_balance: "credit",
        existing: false,
    },
    AccountMapDef {
        qb_name: "Venn:4594 Physical",
        code: "2102",
        name: "Venn CC 4594 Physical",
        account_type: "liability",
        sub_type: "current_liability",
        normal_balance: "credit",
        existing: false,
    },
    AccountMapDef {
        qb_name: "Accounts Receivable (A/R)",
        code: "1100",
        name: "Accounts Receivable",
        account_type: "asset",
        sub_type: "current_asset",
        normal_balance: "debit",
        existing: true,
    },
    AccountMapDef {
        qb_name: "Accounts Receivable (A/R) - USD",
        code: "1101",
        name: "Accounts Receivable (USD)",
        account_type: "asset",
        sub_type: "current_asset",
        normal_balance: "debit",
        existing: false,
    },
    AccountMapDef {
        qb_name: "Accounts Payable (A/P)",
        code: "2000",
        name: "Accounts Payable",
        account_type: "liability",
        sub_type: "current_liability",
        normal_balance: "credit",
        existing: true,
    },
    AccountMapDef {
        qb_name: "Direct Deposit Payable",
        code: "2401",
        name: "Direct Deposit Payable",
        account_type: "liability",
        sub_type: "current_liability",
        normal_balance: "credit",
        existing: false,
    },
    AccountMapDef {
        qb_name: "GST/HST Payable",
        code: "2200",
        name: "GST/HST Payable",
        account_type: "liability",
        sub_type: "current_liability",
        normal_balance: "credit",
        existing: true,
    },
    AccountMapDef {
        qb_name: "Payroll Liabilities:Federal Taxes",
        code: "2402",
        name: "Payroll Liabilities - Federal Taxes",
        account_type: "liability",
        sub_type: "current_liability",
        normal_balance: "credit",
        existing: false,
    },
    AccountMapDef {
        qb_name: "Payroll Liabilities:Vacation Pay",
        code: "2403",
        name: "Payroll Liabilities - Vacation Pay",
        account_type: "liability",
        sub_type: "current_liability",
        normal_balance: "credit",
        existing: false,
    },
    AccountMapDef {
        qb_name: "Opening Balance Equity",
        code: "3001",
        name: "Opening Balance Equity",
        account_type: "equity",
        sub_type: "equity",
        normal_balance: "credit",
        existing: false,
    },
    AccountMapDef {
        qb_name: "GHA Sales",
        code: "4001",
        name: "GHA Sales",
        account_type: "revenue",
        sub_type: "operating_revenue",
        normal_balance: "credit",
        existing: false,
    },
    AccountMapDef {
        qb_name: "Sales",
        code: "4000",
        name: "Service Revenue",
        account_type: "revenue",
        sub_type: "operating_revenue",
        normal_balance: "credit",
        existing: true,
    },
    AccountMapDef {
        qb_name: "Advertising/Promotional",
        code: "6800",
        name: "Marketing & Advertising",
        account_type: "expense",
        sub_type: "operating_expense",
        normal_balance: "debit",
        existing: true,
    },
    AccountMapDef {
        qb_name: "Consulting Fees",
        code: "5101",
        name: "Consulting Fees",
        account_type: "expense",
        sub_type: "cost_of_goods",
        normal_balance: "debit",
        existing: false,
    },
    AccountMapDef {
        qb_name: "Legal and professional fees",
        code: "6700",
        name: "Professional Fees",
        account_type: "expense",
        sub_type: "operating_expense",
        normal_balance: "debit",
        existing: true,
    },
    AccountMapDef {
        qb_name: "Meals and entertainment",
        code: "6901",
        name: "Meals and Entertainment",
        account_type: "expense",
        sub_type: "operating_expense",
        normal_balance: "debit",
        existing: false,
    },
    AccountMapDef {
        qb_name: "Office Expenses",
        code: "6400",
        name: "Office Supplies",
        account_type: "expense",
        sub_type: "operating_expense",
        normal_balance: "debit",
        existing: true,
    },
    AccountMapDef {
        qb_name: "Payroll Expenses:Taxes",
        code: "6200",
        name: "Payroll Taxes",
        account_type: "expense",
        sub_type: "operating_expense",
        normal_balance: "debit",
        existing: true,
    },
    AccountMapDef {
        qb_name: "Payroll Expenses:Wages",
        code: "6000",
        name: "Salaries & Wages",
        account_type: "expense",
        sub_type: "operating_expense",
        normal_balance: "debit",
        existing: true,
    },
    AccountMapDef {
        qb_name: "IRAP Contribution",
        code: "4501",
        name: "IRAP Contribution",
        account_type: "revenue",
        sub_type: "other_revenue",
        normal_balance: "credit",
        existing: false,
    },
    AccountMapDef {
        qb_name: "Exchange Gain or Loss",
        code: "7501",
        name: "Exchange Gain or Loss",
        account_type: "expense",
        sub_type: "other_expense",
        normal_balance: "debit",
        existing: false,
    },
];

struct BankAccountDef {
    name: &'static str,
    institution: &'static str,
    last4: Option<&'static str>,
    currency: &'static str,
    qb_account: &'static str,
}

const BANK_ACCOUNTS: &[BankAccountDef] = &[
    BankAccountDef {
        name: "RBC CAD 7336",
        institution: "Royal Bank of Canada",
        last4: Some("7336"),
        currency: "CAD",
        qb_account: "RBC CAD 7336",
    },
    BankAccountDef {
        name: "RBC USD",
        institution: "Royal Bank of Canada",
        last4: None,
        currency: "USD",
        qb_account: "RBC USD",
    },
    BankAccountDef {
        name: "Venn - 2756",
        institution: "Venn (Neo Financial)",
        last4: Some("2756"),
        currency: "CAD",
        qb_account: "Venn - 2756",
    },
    BankAccountDef {
        name: "Venn - 5516",
        institution: "Venn (Neo Financial)",
        last4: Some("5516"),
        currency: "CAD",
        qb_account: "Venn - 5516",
    },
];
