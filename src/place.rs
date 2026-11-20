//! Where the weather is for. The device's own position (Windows location: Wi-Fi, GPS or cell) with the IP address as
//! fallback, the area there (for Indonesia: kelurahan/desa, kecamatan, kota/kabupaten, provinsi; never the street,
//! RT/RW or postcode) from OpenStreetMap (Nominatim) and BigDataCloud asked at the same time, and online place search.
use crate::{err, AppResult};
use serde_json::{json, Value};
use std::time::Duration;
// What differs per OS lives in win/place.rs and mac/place.rs (picked by cfg(win) from build.rs).
#[cfg(win)]
#[path = "win/place.rs"]
mod os;
#[cfg(not(win))]
#[path = "mac/place.rs"]
mod os;
#[allow(unused_imports)]
pub(crate) use os::*;

fn client(secs: u64) -> AppResult<reqwest::Client> {
    // Nominatim's usage policy asks for an app-specific user agent.
    reqwest::Client::builder().timeout(Duration::from_secs(secs)).user_agent(concat!("Sora/", env!("CARGO_PKG_VERSION"), " (desktop notch weather)")).build().map_err(err)
}
async fn fetch(r: reqwest::RequestBuilder) -> Option<Value> { r.send().await.ok()?.error_for_status().ok()?.json().await.ok() }
/// "id-ID,en" style language list from the page; anything odd becomes English.
fn languages(lang: &str) -> String {
    let ok = !lang.is_empty() && lang.len() <= 40 && lang.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == ',');
    if ok { lang.to_string() } else { "en".into() }
}
fn check(lat: f64, lon: f64) -> AppResult<()> {
    if !lat.is_finite() || !lon.is_finite() || lat.abs() > 90.0 || lon.abs() > 180.0 { return Err("Invalid coordinates".into()); }
    Ok(())
}

async fn ip_position() -> AppResult<(f64, f64, String)> {
    let c = client(5)?;
    for url in ["https://ipwho.is/", "https://ipapi.co/json/"] {
        if let Some(v) = fetch(c.get(url)).await {
            if let (Some(lat), Some(lon)) = (v["latitude"].as_f64(), v["longitude"].as_f64()) { return Ok((lat, lon, v["city"].as_str().unwrap_or("").into())); }
        }
    }
    Err("Location unavailable".into())
}

/// The position to use: Windows location when it is closer than city level, otherwise the IP address's city.
#[tauri::command]
pub async fn auto_location() -> AppResult<Value> {
    let device = tauri::async_runtime::spawn_blocking(device_position).await.ok().flatten();
    if let Some((lat, lon, acc)) = device.filter(|d| check(d.0, d.1).is_ok() && d.2 < 25_000.0) {
        return Ok(json!({ "latitude": lat, "longitude": lon, "accuracy": acc.round(), "source": "device" }));
    }
    let (lat, lon, city) = ip_position().await?;
    check(lat, lon)?;
    Ok(json!({ "latitude": lat, "longitude": lon, "city": city, "source": "ip" }))
}

/// The areas at a position (kelurahan up to country). Fails only when neither provider answers.
#[tauri::command]
pub async fn place_lookup(latitude: f64, longitude: f64, lang: String) -> AppResult<Value> {
    check(latitude, longitude)?;
    let (lang, c) = (languages(&lang), client(6)?);
    let (lat, lon) = (latitude.to_string(), longitude.to_string());
    let nominatim = c.get("https://nominatim.openstreetmap.org/reverse").query(&[("lat", lat.as_str()), ("lon", lon.as_str()), ("format", "jsonv2"), ("zoom", "18"), ("addressdetails", "1"), ("accept-language", lang.as_str())]);
    let primary = lang.split([',', '-']).next().unwrap_or("en");
    let bigdatacloud = c.get("https://api.bigdatacloud.net/data/reverse-geocode-client").query(&[("latitude", lat.as_str()), ("longitude", lon.as_str()), ("localityLanguage", primary)]);
    let (n, b) = futures_util::future::join(fetch(nominatim), fetch(bigdatacloud)).await;
    if n.is_none() && b.is_none() { return Err("Address unavailable".into()); }
    Ok(build_place(&n.unwrap_or(Value::Null), &b.unwrap_or(Value::Null)))
}

/// Online place search (streets, kelurahan, villages, landmarks), next to the bundled city list.
#[tauri::command]
pub async fn place_search(query: String, lang: String) -> AppResult<Vec<Value>> {
    let query = query.trim().to_string();
    if query.chars().count() < 2 || query.chars().count() > 150 { return Ok(vec![]); }
    let lang = languages(&lang);
    let data = fetch(client(5)?.get("https://nominatim.openstreetmap.org/search").query(&[("q", query.as_str()), ("format", "jsonv2"), ("addressdetails", "1"), ("limit", "8"), ("accept-language", lang.as_str())])).await.ok_or("Search unavailable")?;
    Ok(data.as_array().into_iter().flatten().filter_map(|p| {
        let (lat, lon) = (p["lat"].as_str()?.parse::<f64>().ok()?, p["lon"].as_str()?.parse::<f64>().ok()?);
        check(lat, lon).ok()?;
        let full = p["display_name"].as_str().unwrap_or("");
        let name = p["name"].as_str().filter(|s| !s.is_empty()).or(full.split(", ").next())?;
        let detail = full.split(", ").filter(|s| *s != name).collect::<Vec<_>>().join(", ");
        Some(json!({ "name": name, "detail": detail, "latitude": lat, "longitude": lon, "online": true }))
    }).collect())
}

fn same(a: &str, b: &str) -> bool { a.trim().eq_ignore_ascii_case(b.trim()) }
/// Nominatim's and BigDataCloud's answers as one address, finest part first:
/// { name, area, parts: [{ label, name }], country_code }, areas only (no street, RT/RW or postcode).
pub fn build_place(n: &Value, b: &Value) -> Value {
    let a = &n["address"];
    let s = |k: &str| a[k].as_str().filter(|v| !v.is_empty());
    let cc = s("country_code").map(str::to_lowercase).or(b["countryCode"].as_str().map(str::to_lowercase)).unwrap_or_default();
    let indonesia = cc == "id";
    let display: Vec<&str> = n["display_name"].as_str().unwrap_or("").split(", ").filter(|p| !p.is_empty()).collect();
    let mut parts: Vec<(String, String)> = Vec::new();
    // Indonesian areas may share a name across levels (Kota and Provinsi Yogyakarta, Kelurahan and Kecamatan Cibinong).
    const ID_AREAS: [&str; 7] = ["Kelurahan", "Desa", "Kelurahan/Desa", "Kecamatan", "Kota", "Kabupaten", "Provinsi"];
    let push = |parts: &mut Vec<(String, String)>, label: &str, name: &str| {
        let dup = parts.iter().any(|p| same(&p.1, name) && !(p.0 != label && ID_AREAS.contains(&p.0.as_str()) && ID_AREAS.contains(&label)));
        if !name.is_empty() && !dup { parts.push((label.into(), name.into())); }
    };

    // BigDataCloud lists the administrative areas coarse to fine; levels 2 (country) and 3 (island) are left out.
    let admins: Vec<(u64, &str, String)> = b["localityInfo"]["administrative"].as_array().into_iter().flatten().filter_map(|x| {
        let level = x["adminLevel"].as_u64()?;
        (level >= 4).then(|| (level, x["name"].as_str().unwrap_or(""), x["description"].as_str().unwrap_or("").to_lowercase()))
    }).filter(|x| !x.1.is_empty()).collect();
    // Its descriptions come from Wikidata ("kota di D.I Yogyakarta", "kecamatan di Kabupaten Bogor"); the level is the fallback.
    // ("ibu kota provinsi ..." describes a kota, so only the start of the description counts.)
    let id_label = |level: u64, d: &str| match level {
        4 if d.starts_with("kota") => "Kota",
        4 => "Provinsi",
        5 if d.starts_with("kabupaten") => "Kabupaten",
        5 => "Kota",
        6 => "Kecamatan",
        _ if d.starts_with("desa") => "Desa",
        _ if d.starts_with("kelurahan") => "Kelurahan",
        _ => "Kelurahan/Desa",
    };

    if indonesia {
        // One area per label; on a border BigDataCloud may give two kecamatan, so keep the one in OSM's address line.
        let mut areas: Vec<(&str, &str)> = Vec::new();
        for x in admins.iter().rev() {
            let label = id_label(x.0, &x.2);
            match areas.iter().position(|a| a.0 == label) {
                Some(i) => if !display.iter().any(|p| same(p, areas[i].1)) && display.iter().any(|p| same(p, x.1)) { areas[i].1 = x.1; },
                None => areas.push((label, x.1)),
            }
        }
        if !areas.iter().any(|a| ["Kelurahan", "Desa", "Kelurahan/Desa"].contains(&a.0)) {
            // OSM's "village", else the part of its address line just finer than the kecamatan (Jakarta tags the
            // kelurahan "neighbourhood"), skipping RT/RW, streets, housing estates and house numbers.
            let kecamatan = areas.iter().find(|a| a.0 == "Kecamatan").map(|a| a.1).or(s("suburb")).or(s("city_district"));
            let not_area = ["road", "residential", "house_number", "house_name", "amenity", "building", "shop", "tourism", "leisure", "office", "industrial", "commercial", "retail", "hamlet", "city_block", "place"];
            let skip = |p: &str| p.starts_with("RW ") || p.starts_with("RT ") || p.chars().next().is_some_and(|c| c.is_ascii_digit()) || not_area.iter().any(|k| s(k).is_some_and(|v| same(v, p)));
            let by_line = kecamatan.and_then(|k| display.iter().position(|p| same(p, k))).and_then(|i| display[..i].iter().rev().find(|p| !skip(p)).copied());
            let kelurahan = s("village").filter(|v| !skip(v)).or(by_line).or(s("neighbourhood").filter(|v| !skip(v) && kecamatan.is_some_and(|k| !same(k, v))));
            // DKI Jakarta has no desa; elsewhere OSM does not say which of the two it is.
            let jakarta = s("ISO3166-2-lvl4") == Some("ID-JK");
            if let Some(v) = kelurahan { push(&mut parts, if jakarta { "Kelurahan" } else { "Kelurahan/Desa" }, v); }
        }
        for (label, name) in &areas { push(&mut parts, label, name); }
        if areas.is_empty() {
            // BigDataCloud did not answer: Nominatim's own keys.
            for (k, label) in [("suburb", "Kecamatan"), ("city_district", "Kecamatan"), ("municipality", "Kecamatan"), ("city", "Kota"), ("town", "Kota"), ("county", "Kabupaten"), ("state", "Provinsi")] {
                if let Some(v) = s(k) { push(&mut parts, label, v); }
            }
        }
    } else {
        for (k, label) in [("neighbourhood", "Neighbourhood"), ("quarter", "Neighbourhood"), ("suburb", "Suburb"), ("village", "Village"), ("hamlet", "Village"), ("city_district", "District"), ("district", "District"), ("borough", "District"), ("town", "City"), ("city", "City"), ("municipality", "City"), ("county", "County"), ("state_district", "Region"), ("state", "State"), ("province", "State"), ("region", "Region")] {
            if let Some(v) = s(k) { push(&mut parts, label, v); }
        }
        for x in admins.iter().rev() { push(&mut parts, "Area", x.1); }
    }
    if let Some(country) = s("country").or(b["countryName"].as_str().filter(|c| !c.is_empty())) { push(&mut parts, if indonesia { "Negara" } else { "Country" }, country); }

    // The short name is the finest area (not the street); the area line is the two above it.
    let areas: Vec<&(String, String)> = parts.iter().filter(|p| !["Negara", "Country"].contains(&p.0.as_str())).collect();
    let name = areas.first().map(|p| p.1.clone()).or(b["locality"].as_str().map(String::from)).or(b["city"].as_str().map(String::from)).unwrap_or_default();
    let area = areas.iter().skip(1).take(2).map(|p| p.1.as_str()).collect::<Vec<_>>().join(", ");
    json!({
        "name": name,
        "area": area,
        "parts": parts.iter().map(|p| json!({ "label": p.0, "name": p.1 })).collect::<Vec<_>>(),
        "country_code": cc,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn names(v: &Value) -> Vec<String> { v["parts"].as_array().unwrap().iter().map(|p| format!("{}={}", p["label"].as_str().unwrap(), p["name"].as_str().unwrap())).collect() }

    #[test]
    fn jakarta_gets_kelurahan_from_the_address_line() {
        let n = json!({ "display_name": "Jalan Balai Rakyat I, RW 01, Pondok Bambu, Duren Sawit, Jakarta Timur, Daerah Khusus Ibukota Jakarta, 13470, Indonesia",
            "address": { "road": "Jalan Balai Rakyat I", "city_block": "RW 01", "neighbourhood": "Pondok Bambu", "suburb": "Duren Sawit", "city_district": "Jakarta Timur", "city": "Daerah Khusus Ibukota Jakarta", "ISO3166-2-lvl4": "ID-JK", "postcode": "13470", "country": "Indonesia", "country_code": "id" } });
        let b = json!({ "countryCode": "ID", "localityInfo": { "administrative": [
            { "name": "Indonesia", "adminLevel": 2 }, { "name": "Jawa", "adminLevel": 3, "description": "pulau di Indonesia" },
            { "name": "Jakarta", "adminLevel": 4, "description": "ibu kota dan provinsi" }, { "name": "Jakarta", "adminLevel": 4 },
            { "name": "Jakarta Timur", "adminLevel": 5, "description": "kota di Provinsi Jakarta" },
            { "name": "Duren Sawit", "adminLevel": 6, "description": "kecamatan di Kota Administrasi Jakarta Timur" } ] } });
        let p = build_place(&n, &b);
        assert_eq!(names(&p), ["Kelurahan=Pondok Bambu", "Kecamatan=Duren Sawit", "Kota=Jakarta Timur", "Provinsi=Jakarta", "Negara=Indonesia"]);
        assert_eq!(p["name"], "Pondok Bambu");
        assert_eq!(p["area"], "Duren Sawit, Jakarta Timur");
    }

    #[test]
    fn regency_and_desa_from_bigdatacloud_levels() {
        let b = json!({ "countryCode": "ID", "countryName": "Indonesia", "localityInfo": { "administrative": [
            { "name": "Jawa Barat", "adminLevel": 4, "description": "provinsi di Indonesia" },
            { "name": "Bogor", "adminLevel": 5, "description": "kabupaten di Jawa Barat" },
            { "name": "Cibinong", "adminLevel": 6, "description": "kecamatan di Kabupaten Bogor" },
            { "name": "Pakansari", "adminLevel": 7, "description": "desa di Cibinong" } ] } });
        let p = build_place(&Value::Null, &b);
        assert_eq!(names(&p), ["Desa=Pakansari", "Kecamatan=Cibinong", "Kabupaten=Bogor", "Provinsi=Jawa Barat", "Negara=Indonesia"]);
        assert_eq!(p["name"], "Pakansari");
    }

    #[test]
    fn border_keeps_the_kecamatan_in_the_address_line_and_skips_estates() {
        let n = json!({ "display_name": "Jalan Kanginan I, RW 01, Ketabang, Genteng, Surabaya, Jawa Timur, 60272, Indonesia",
            "address": { "road": "Jalan Kanginan I", "neighbourhood": "RW 01", "village": "Ketabang", "municipality": "Genteng", "city": "Surabaya", "state": "Jawa Timur", "postcode": "60272", "country": "Indonesia", "country_code": "id" } });
        let b = json!({ "localityInfo": { "administrative": [
            { "name": "Jawa Timur", "adminLevel": 4, "description": "provinsi di Pulau Jawa" }, { "name": "Surabaya", "adminLevel": 5, "description": "kota di Provinsi Jawa Timur" },
            { "name": "Tambaksari", "adminLevel": 6, "description": "kecamatan di Kota Surabaya" }, { "name": "Genteng", "adminLevel": 6, "description": "kecamatan di Kota Surabaya" } ] } });
        assert_eq!(names(&build_place(&n, &b)), ["Kelurahan/Desa=Ketabang", "Kecamatan=Genteng", "Kota=Surabaya", "Provinsi=Jawa Timur", "Negara=Indonesia"]);

        let n = json!({ "display_name": "Gang R. Winasis, Komplek A, Suryatmajan, Danurejan, Kota Yogyakarta, Daerah Istimewa Yogyakarta, 55213, Indonesia",
            "address": { "road": "Gang R. Winasis", "residential": "Komplek A", "neighbourhood": "Suryatmajan", "city_district": "Danurejan", "country_code": "id" } });
        let b = json!({ "localityInfo": { "administrative": [
            { "name": "Yogyakarta", "adminLevel": 4, "description": "daerah istimewa di Pulau Jawa" }, { "name": "Yogyakarta", "adminLevel": 4, "description": "kota di D.I Yogyakarta" },
            { "name": "Danurejan", "adminLevel": 6, "description": "kecamatan di Kota Yogyakarta" } ] } });
        assert_eq!(names(&build_place(&n, &b)), ["Kelurahan/Desa=Suryatmajan", "Kecamatan=Danurejan", "Kota=Yogyakarta", "Provinsi=Yogyakarta"]);
    }

    #[test]
    fn other_countries_use_nominatim_keys() {
        let n = json!({ "display_name": "Shibuya, Tokyo, Japan", "address": { "suburb": "Shibuya", "city": "Tokyo", "postcode": "150-0002", "country": "Japan", "country_code": "jp" } });
        let p = build_place(&n, &Value::Null);
        assert_eq!(names(&p), ["Suburb=Shibuya", "City=Tokyo", "Country=Japan"]);
        assert_eq!(p["area"], "Tokyo");
    }

    #[test]
    fn odd_languages_fall_back_to_english() {
        assert_eq!(languages("id-ID,en"), "id-ID,en");
        assert_eq!(languages("x&y=1"), "en");
        assert_eq!(languages(""), "en");
    }
}
