use anyhow::{anyhow, Context, Result};
use chrono::{Duration as ChronoDuration, SecondsFormat, Utc};
use std::{collections::BTreeMap, sync::{Arc, Mutex}, time::Duration};
use tokio::time::sleep;
use url::Url;

use crate::types::{AppState, HorizonsJson, Vec3};

pub fn build_horizons_url(body_id: &str, start_utc: &str, stop_utc: &str) -> Result<Url> {
    let mut url = Url::parse("https://ssd.jpl.nasa.gov/api/horizons.api")?;
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("format", "json");
        qp.append_pair("MAKE_EPHEM", "YES");
        qp.append_pair("OBJ_DATA", "NO");
        qp.append_pair("EPHEM_TYPE", "VECTORS");

        qp.append_pair("COMMAND", body_id);
        qp.append_pair("CENTER", "500@10");
        qp.append_pair("REF_PLANE", "ECLIPTIC");
        qp.append_pair("REF_SYSTEM", "ICRF");
        qp.append_pair("OUT_UNITS", "AU-D");
        qp.append_pair("CSV_FORMAT", "YES");
        qp.append_pair("VEC_TABLE", "1");
        qp.append_pair("TIME_TYPE", "UT");

        qp.append_pair("START_TIME", &format!("'{}'", start_utc));
        qp.append_pair("STOP_TIME", &format!("'{}'", stop_utc));
        qp.append_pair("STEP_SIZE", "'1 m'");
    }
    Ok(url)
}

pub fn extract_table_lines(result_text: &str) -> Result<Vec<&str>> {
    let so = result_text.find("$$SOE").ok_or_else(|| anyhow!("Missing $$SOE marker"))?;
    let eo = result_text.find("$$EOE").ok_or_else(|| anyhow!("Missing $$EOE marker"))?;
    if eo <= so {
        return Err(anyhow!("$$EOE occurs before $$SOE"));
    }
    let table = &result_text[(so + 5)..eo];
    Ok(table.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect())
}

pub fn parse_xyz_from_csv_row(row: &str) -> Result<Vec3> {
    let cols: Vec<&str> = row
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    if cols.len() < 5 {
        return Err(anyhow!("Unexpected CSV format: {}", row));
    }

    let x = cols[cols.len() - 3].parse::<f64>().context("parse x")?;
    let y = cols[cols.len() - 2].parse::<f64>().context("parse y")?;
    let z = cols[cols.len() - 1].parse::<f64>().context("parse z")?;
    Ok(Vec3 { x, y, z })
}

pub async fn fetch_body_vec(client: &reqwest::Client, body_id: &str, start_utc: &str, stop_utc: &str) -> Result<Vec3> {
    let url = build_horizons_url(body_id, start_utc, stop_utc)?;
    let body = client.get(url).send().await?.error_for_status()?.text().await?;
    let parsed: HorizonsJson = serde_json::from_str(&body).context("parse Horizons JSON")?;
    if let Some(e) = parsed.error {
        return Err(anyhow!("Horizons error: {}", e));
    }
    let lines = extract_table_lines(&parsed.result)?;
    for line in lines {
        if let Ok(v) = parse_xyz_from_csv_row(line) {
            return Ok(v);
        }
    }
    Err(anyhow!("No parseable vector row for body {}", body_id))
}

pub async fn updater(state: Arc<Mutex<AppState>>) {
    let client = reqwest::Client::builder()
        .user_agent("solar-rs/0.5 (ratatui)")
        .build()
        .expect("reqwest client");

    loop {
        let now_label = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);

        let start = Utc::now();
        let stop = start + ChronoDuration::minutes(1);
        let start_str = start.format("%Y-%b-%d %H:%M:%S").to_string();
        let stop_str = stop.format("%Y-%b-%d %H:%M:%S").to_string();

        let bodies_snapshot = {
            let s = state.lock().unwrap();
            s.bodies.iter().filter(|b| b.id != "10").map(|b| (b.name, b.id)).collect::<Vec<_>>()
        };

        let mut new_positions: BTreeMap<&'static str, Vec3> = BTreeMap::new();
        let mut status = "OK".to_string();

        for (name, id) in bodies_snapshot {
            match fetch_body_vec(&client, id, &start_str, &stop_str).await {
                Ok(v) => { new_positions.insert(name, v); }
                Err(e) => status = format!("Fetch error ({}): {}", name, e),
            }
            sleep(Duration::from_millis(120)).await;
        }

        {
            let mut s = state.lock().unwrap();
            for b in &mut s.bodies {
                if b.id == "10" {
                    b.pos_au = Some(Vec3 { x: 0.0, y: 0.0, z: 0.0 });
                } else {
                    b.pos_au = new_positions.get(b.name).copied().or(b.pos_au);
                }
            }
            s.last_update_utc = Some(now_label);
            s.status = status;
        }

        sleep(Duration::from_secs(5)).await;
    }
}
