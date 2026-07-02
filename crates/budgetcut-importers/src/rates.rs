//! Live market-data parsers (FX + fuel), **pure string → struct** so the
//! fetching side (server endpoint, Tauri command) stays a thin I/O shell and
//! the parsing is unit-testable offline.
//!
//! Sources:
//! * **TCMB** `kurlar/today.xml` — official central-bank daily FX (döviz satış).
//! * **Opet** public fuel-price API — İstanbul pump prices per district.
//! * **hasanadiguzel.com.tr** akaryakıt API — fallback fuel source.

use serde_json::Value;

/// Parsed live rates; every field optional so partial failures degrade to "—".
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct LiveRates {
    /// TCMB bulletin date, e.g. "01.07.2026".
    pub date: Option<String>,
    /// USD/TRY döviz satış (forex selling), e.g. "46.6706".
    pub usd: Option<String>,
    /// EUR/TRY döviz satış.
    pub eur: Option<String>,
    /// İstanbul Kurşunsuz 95 pump price, ₺/lt, e.g. "62.64".
    pub benzin: Option<String>,
    /// İstanbul motorin (diesel) pump price, ₺/lt.
    pub motorin: Option<String>,
}

/// Extract `<Tag>value</Tag>` inside the `Kod="XXX"` currency block. TCMB's XML
/// is flat and stable; plain string scanning avoids an XML-namespace dance.
fn tcmb_field(xml: &str, code: &str, tag: &str) -> Option<String> {
    let block_start = xml.find(&format!("Kod=\"{code}\""))?;
    let rest = &xml[block_start..];
    let end = rest.find("</Currency>").unwrap_or(rest.len());
    let block = &rest[..end];
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let s = block.find(&open)? + open.len();
    let e = block[s..].find(&close)? + s;
    let v = block[s..e].trim();
    if v.is_empty() {
        None
    } else {
        Some(v.to_string())
    }
}

/// Parse TCMB `today.xml` → (date, USD selling, EUR selling).
#[must_use]
pub fn parse_tcmb_xml(xml: &str) -> (Option<String>, Option<String>, Option<String>) {
    let date = xml
        .find("Tarih=\"")
        .and_then(|i| {
            let s = i + "Tarih=\"".len();
            xml[s..].find('"').map(|e| xml[s..s + e].to_string())
        })
        .filter(|d| !d.is_empty());
    let usd = tcmb_field(xml, "USD", "ForexSelling");
    let eur = tcmb_field(xml, "EUR", "ForexSelling");
    (date, usd, eur)
}

/// Parse the Opet fuel-price JSON (array of districts, each with `prices`).
/// Returns (benzin, motorin) from the first district carrying both.
#[must_use]
pub fn parse_opet_json(json: &str) -> (Option<String>, Option<String>) {
    let v: Value = match serde_json::from_str(json) {
        Ok(v) => v,
        Err(_) => return (None, None),
    };
    let districts = match v.as_array() {
        Some(a) => a,
        None => return (None, None),
    };
    for d in districts {
        let mut benzin = None;
        let mut motorin = None;
        for p in d["prices"].as_array().unwrap_or(&Vec::new()) {
            let name = p["productName"].as_str().unwrap_or("");
            let amount = &p["amount"];
            let val = amount
                .as_f64()
                .map(|f| format!("{f:.2}"))
                .or_else(|| amount.as_str().map(|s| s.to_string()));
            if name.contains("Kurşunsuz Benzin 95") || name.contains("Kurşunsuz 95") {
                benzin = benzin.or(val.clone());
            } else if name.contains("Motorin") && !name.contains("Ultra") {
                motorin = motorin.or(val);
            }
        }
        if benzin.is_some() {
            return (benzin, motorin);
        }
    }
    (None, None)
}

/// Parse the hasanadiguzel akaryakıt JSON (fallback): `data` is a map whose
/// values hold `Kursunsuz_95(...)_TL/lt` / `Motorin(...)_TL/lt` fields with
/// Turkish decimal commas.
#[must_use]
pub fn parse_ha_json(json: &str) -> (Option<String>, Option<String>) {
    let v: Value = match serde_json::from_str(json) {
        Ok(v) => v,
        Err(_) => return (None, None),
    };
    let data = match v["data"].as_object() {
        Some(o) => o,
        None => return (None, None),
    };
    for entry in data.values() {
        if let Some(o) = entry.as_object() {
            let mut benzin = None;
            let mut motorin = None;
            for (k, val) in o {
                let val = val.as_str().map(|s| s.replace(',', "."));
                if k.starts_with("Kursunsuz_95") {
                    benzin = benzin.or(val.clone());
                } else if k.starts_with("Motorin") {
                    motorin = motorin.or(val);
                }
            }
            if benzin.is_some() {
                return (benzin, motorin);
            }
        }
    }
    (None, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tcmb_parses_date_and_selling_rates() {
        let xml = r#"<?xml version="1.0" encoding="ISO-8859-9"?>
<Tarih_Date Tarih="01.07.2026" Date="07/01/2026">
  <Currency CrossOrder="0" Kod="USD" CurrencyCode="USD">
    <Unit>1</Unit><ForexBuying>46.5866</ForexBuying><ForexSelling>46.6706</ForexSelling>
  </Currency>
  <Currency CrossOrder="9" Kod="EUR" CurrencyCode="EUR">
    <Unit>1</Unit><ForexBuying>53.0891</ForexBuying><ForexSelling>53.1847</ForexSelling>
  </Currency>
</Tarih_Date>"#;
        let (date, usd, eur) = parse_tcmb_xml(xml);
        assert_eq!(date.as_deref(), Some("01.07.2026"));
        assert_eq!(usd.as_deref(), Some("46.6706"));
        assert_eq!(eur.as_deref(), Some("53.1847"));
    }

    #[test]
    fn opet_parses_first_district_prices() {
        let json = r#"[{"provinceName":"İSTANBUL ANADOLU","districtName":"ADALAR","prices":[
            {"productName":"Kurşunsuz Benzin 95","amount":62.64},
            {"productName":"Motorin EcoForce","amount":64.51}]}]"#;
        let (b, m) = parse_opet_json(json);
        assert_eq!(b.as_deref(), Some("62.64"));
        assert_eq!(m.as_deref(), Some("64.51"));
    }

    #[test]
    fn ha_fallback_parses_comma_decimals() {
        let json = r#"{"data":{"62,73":{"Kursunsuz_95(Excellium95)_TL/lt":"69,35","Motorin(Eurodiesel)_TL/lt":"64,53"}}}"#;
        let (b, m) = parse_ha_json(json);
        assert_eq!(b.as_deref(), Some("69.35"));
        assert_eq!(m.as_deref(), Some("64.53"));
    }

    #[test]
    fn garbage_degrades_to_none_not_panic() {
        assert_eq!(parse_tcmb_xml("not xml"), (None, None, None));
        assert_eq!(parse_opet_json("]["), (None, None));
        assert_eq!(parse_ha_json("{}"), (None, None));
    }
}
