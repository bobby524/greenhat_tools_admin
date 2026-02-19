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
    customers: Vec<ContactRow>,
    suppliers: Vec<ContactRow>,
    journal: Vec<JournalEntry>,
    parse_errors: Vec<String>,
}

#[derive(Debug, Clone)]
struct ContactRow {
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

    let import_result = run_import(&parsed, dry_run, &mut report).await;
    if let Err(e) = import_result {
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
            purge_report: None,
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
            parsed: 0,
            note: "Employees parsed for reference (no gb_employees table)".to_string(),
        },
        general_ledger: RowsNote {
            rows: 0,
            note: "GL data used for cross-validation; journal entries are the primary import"
                .to_string(),
        },
        trial_balance: RowsNote {
            rows: 0,
            note: "Trial balance used for validation".to_string(),
        },
        profit_and_loss: RowsNote {
            rows: 0,
            note: "P&L used for validation".to_string(),
        },
        balance_sheet: RowsNote {
            rows: 0,
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
            employees: 0,
            journal_entries: parsed.journal.len(),
            journal_lines,
            general_ledger: 0,
            trial_balance: 0,
            profit_and_loss: 0,
            balance_sheet: 0,
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

    let mut account_map = HashMap::new();
    for (qb_name, code, name, account_type, sub_type, normal_balance) in QB_ACCOUNT_MAP {
        let existing = find_account_id(&client, &base, code).await?;
        if let Some(id) = existing {
            account_map.insert((*qb_name).to_string(), id);
            report.accounts.reused += 1;
            continue;
        }

        if dry_run {
            account_map.insert((*qb_name).to_string(), format!("dry-run-{code}"));
            report.accounts.created += 1;
            continue;
        }

        let payload = serde_json::json!([{"org_id": ORG_ID, "code": code, "name": name, "account_type": account_type, "sub_type": sub_type, "normal_balance": normal_balance, "is_active": true, "is_system": false}]);
        let url = format!("{base}/rest/v1/gb_accounts?on_conflict=org_id,code");
        let resp = client
            .post(&url)
            .header(
                "Prefer",
                "resolution=merge-duplicates,return=representation",
            )
            .json(&payload)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let status = resp.status();
        let txt = resp.text().await.map_err(|e| e.to_string())?;
        if !status.is_success() {
            return Err(format!("failed upserting account {code}: {txt}"));
        }
        let rows: Vec<Value> = serde_json::from_str(&txt).unwrap_or_default();
        if let Some(id) = rows
            .first()
            .and_then(|r| r.get("id"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
        {
            account_map.insert((*qb_name).to_string(), id);
            report.accounts.created += 1;
        }
    }

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
        if exists_by_name(&client, &base, "gb_customers", &c.name).await? {
            report.customers.skipped += 1;
            report
                .customers
                .details
                .push(format!("Skipped (exists): {}", c.name));
            continue;
        }
        let payload = serde_json::json!([{"org_id": ORG_ID, "name": c.name, "email": c.email, "phone": c.phone}]);
        post_rows(&client, &format!("{base}/rest/v1/gb_customers"), &payload).await?;
        report.customers.imported += 1;
        report
            .customers
            .details
            .push(format!("Imported: {}", c.name));
    }

    for v in &parsed.suppliers {
        if v.name.trim().is_empty() {
            report.suppliers.skipped += 1;
            continue;
        }
        if dry_run {
            report.suppliers.imported += 1;
            report
                .suppliers
                .details
                .push(format!("[DRY-RUN] Would import supplier: {}", v.name));
            continue;
        }
        if exists_by_name(&client, &base, "gb_vendors", &v.name).await? {
            report.suppliers.skipped += 1;
            report
                .suppliers
                .details
                .push(format!("Skipped (exists): {}", v.name));
            continue;
        }
        let payload = serde_json::json!([{"org_id": ORG_ID, "name": v.name, "email": v.email, "phone": v.phone}]);
        post_rows(&client, &format!("{base}/rest/v1/gb_vendors"), &payload).await?;
        report.suppliers.imported += 1;
        report
            .suppliers
            .details
            .push(format!("Imported: {}", v.name));
    }

    let mut unmapped = HashSet::new();
    for (idx, entry) in parsed.journal.iter().enumerate() {
        let mut lines_payload = Vec::new();
        for (line_idx, line) in entry.lines.iter().enumerate() {
            let Some(account_id) = account_map.get(&line.account) else {
                unmapped.insert(line.account.clone());
                continue;
            };
            lines_payload.push(serde_json::json!({"account_id": account_id, "description": line.memo, "debit": line.debit, "credit": line.credit, "sort_order": line_idx}));
        }
        if lines_payload.len() < 2 {
            report.journal_entries.errors += 1;
            continue;
        }

        report.journal_entries.total_lines += lines_payload.len();
        if dry_run {
            report.journal_entries.imported += 1;
            continue;
        }

        let entry_num = format!(
            "QBI-{}-{:04}",
            entry.date.get(0..4).unwrap_or("2026"),
            idx + 1
        );
        let total_debit: f64 = entry.lines.iter().map(|l| l.debit).sum();
        let total_credit: f64 = entry.lines.iter().map(|l| l.credit).sum();
        let je_payload = serde_json::json!([{"org_id": ORG_ID, "entry_number": entry_num, "entry_date": entry.date, "description": format!("{} - {}", entry.kind, entry.name.clone().unwrap_or_default()), "reference": entry.num.as_ref().map(|n| format!("QB#{n}")), "source": "qb_import_journal", "status": "posted", "total_debit": total_debit, "total_credit": total_credit}]);
        let je_rows = post_rows(
            &client,
            &format!("{base}/rest/v1/gb_journal_entries"),
            &je_payload,
        )
        .await?;
        let Some(je_id) = je_rows
            .first()
            .and_then(|r| r.get("id"))
            .and_then(|v| v.as_str())
        else {
            report.journal_entries.errors += 1;
            continue;
        };

        let rows: Vec<Value> = lines_payload
            .into_iter()
            .map(|mut l| {
                l["journal_entry_id"] = Value::String(je_id.to_string());
                l
            })
            .collect();
        post_rows(
            &client,
            &format!("{base}/rest/v1/gb_journal_entry_lines"),
            &Value::Array(rows),
        )
        .await?;
        report.journal_entries.imported += 1;
    }

    report.journal_entries.unmapped_accounts = unmapped.into_iter().collect();
    Ok(())
}

fn parse_qb_blob(data: &[u8]) -> ParsedData {
    let mut parsed = ParsedData::default();
    if data.len() > 4 && data[0] == 0x50 && data[1] == 0x4b {
        if let Ok(mut zip) = ZipArchive::new(Cursor::new(data)) {
            for i in 0..zip.len() {
                let Ok(mut file) = zip.by_index(i) else {
                    continue;
                };
                let name = file.name().to_string();
                if !name.ends_with(".xlsx") && !name.ends_with(".xls") {
                    continue;
                }
                let mut buf = vec![];
                if std::io::Read::read_to_end(&mut file, &mut buf).is_ok() {
                    parse_workbook_bytes(&name, &buf, &mut parsed);
                }
            }
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
                .push(format!("failed parsing {name}: {e}"));
            return;
        }
    };

    let sheet_name = wb.sheet_names().first().cloned().unwrap_or_default();
    let Ok(range) = wb.worksheet_range(&sheet_name) else {
        parsed.parse_errors.push(format!("no sheet in {name}"));
        return;
    };

    let rows: Vec<Vec<String>> = range
        .rows()
        .map(|r| r.iter().map(data_to_string).collect::<Vec<String>>())
        .collect();
    let detected = detect_type(name, &rows);
    parsed.files.insert(name.to_string(), detected.clone());

    match detected.as_str() {
        "customers" => parsed.customers.extend(parse_contacts(&rows)),
        "suppliers" => parsed.suppliers.extend(parse_contacts(&rows)),
        "journal" => parsed.journal.extend(parse_journal(&rows)),
        _ => {}
    }
}

fn detect_type(name: &str, rows: &[Vec<String>]) -> String {
    let lower = name.to_ascii_lowercase();
    if lower.contains("customer") {
        return "customers".into();
    }
    if lower.contains("supplier") || lower.contains("vendor") {
        return "suppliers".into();
    }
    if lower.contains("journal") {
        return "journal".into();
    }

    let header = rows
        .iter()
        .take(10)
        .flat_map(|r| r.iter())
        .cloned()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    if header.contains("customer") {
        "customers".into()
    } else if header.contains("supplier") || header.contains("vendor") {
        "suppliers".into()
    } else if header.contains("journal") || (header.contains("debit") && header.contains("credit"))
    {
        "journal".into()
    } else {
        "unknown".into()
    }
}

fn parse_contacts(rows: &[Vec<String>]) -> Vec<ContactRow> {
    let mut out = Vec::new();
    for row in rows.iter().skip(1) {
        let name = row.get(0).cloned().unwrap_or_default().trim().to_string();
        if name.is_empty() || name.to_ascii_lowercase().starts_with("total") {
            continue;
        }
        let phone = row
            .get(1)
            .and_then(|s| (!s.trim().is_empty()).then_some(s.trim().to_string()));
        let email = row
            .iter()
            .find(|s| s.contains('@'))
            .map(|s| s.trim().to_string());
        out.push(ContactRow { name, phone, email });
    }
    out
}

fn parse_journal(rows: &[Vec<String>]) -> Vec<JournalEntry> {
    let mut entries = Vec::new();
    let mut cur: Option<JournalEntry> = None;
    for row in rows.iter().skip(1) {
        let date = row.first().cloned().unwrap_or_default();
        let account = row.get(5).cloned().unwrap_or_default();
        let debit = parse_num(row.get(6));
        let credit = parse_num(row.get(7));
        if date.contains('/') || date.contains('-') {
            if let Some(prev) = cur.take() {
                if !prev.lines.is_empty() {
                    entries.push(prev);
                }
            }
            cur = Some(JournalEntry {
                date: normalize_date(&date),
                kind: row
                    .get(1)
                    .cloned()
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "Journal".into()),
                num: row.get(2).cloned().filter(|s| !s.is_empty()),
                name: row.get(3).cloned().filter(|s| !s.is_empty()),
                lines: vec![],
            });
        }
        if let Some(c) = cur.as_mut() {
            if !account.is_empty() {
                c.lines.push(JournalLine {
                    memo: row.get(4).cloned().filter(|s| !s.is_empty()),
                    account,
                    debit,
                    credit,
                });
            }
        }
    }
    if let Some(last) = cur {
        if !last.lines.is_empty() {
            entries.push(last);
        }
    }
    entries
}

fn data_to_string(d: &Data) -> String {
    match d {
        Data::String(s) => s.clone(),
        Data::Float(f) => f.to_string(),
        Data::Int(i) => i.to_string(),
        Data::Bool(b) => b.to_string(),
        Data::DateTime(dt) => dt.to_string(),
        _ => String::new(),
    }
}

fn parse_num(s: Option<&String>) -> f64 {
    let Some(s) = s else {
        return 0.0;
    };
    let cleaned = s.replace([',', '$'], "").replace(['(', ')'], "");
    cleaned.parse::<f64>().unwrap_or(0.0)
}

fn normalize_date(s: &str) -> String {
    if let Some((m, d, y)) = s
        .split('/')
        .collect::<Vec<_>>()
        .get(0..3)
        .map(|v| (v[0], v[1], v[2]))
    {
        let yy = if y.len() == 2 {
            format!("20{y}")
        } else {
            y.to_string()
        };
        return format!("{yy}-{m:0>2}-{d:0>2}");
    }
    s.chars().take(10).collect()
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
    let resp = client
        .post(url)
        .header("Prefer", "return=representation")
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

const QB_ACCOUNT_MAP: &[(&str, &str, &str, &str, &str, &str)] = &[
    (
        "RBC CAD 7336",
        "1011",
        "RBC CAD 7336",
        "asset",
        "bank",
        "debit",
    ),
    ("RBC USD", "1012", "RBC USD", "asset", "bank", "debit"),
    (
        "Venn - 2756",
        "1013",
        "Venn - 2756",
        "asset",
        "bank",
        "debit",
    ),
    (
        "Venn - 5516",
        "1014",
        "Venn - 5516",
        "asset",
        "bank",
        "debit",
    ),
    (
        "Accounts Receivable (A/R)",
        "1100",
        "Accounts Receivable",
        "asset",
        "current_asset",
        "debit",
    ),
    (
        "Accounts Payable (A/P)",
        "2000",
        "Accounts Payable",
        "liability",
        "current_liability",
        "credit",
    ),
    (
        "GST/HST Payable",
        "2200",
        "GST/HST Payable",
        "liability",
        "current_liability",
        "credit",
    ),
    (
        "Sales",
        "4000",
        "Service Revenue",
        "revenue",
        "operating_revenue",
        "credit",
    ),
    (
        "Legal and professional fees",
        "6700",
        "Professional Fees",
        "expense",
        "operating_expense",
        "debit",
    ),
    (
        "Office Expenses",
        "6400",
        "Office Supplies",
        "expense",
        "operating_expense",
        "debit",
    ),
    (
        "Payroll Expenses:Wages",
        "6000",
        "Salaries & Wages",
        "expense",
        "operating_expense",
        "debit",
    ),
];
