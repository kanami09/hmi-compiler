//! 链接缓存只保存已校验的产物；检查和警告策略每次根据当前工程重新执行。
use super::{Prepared, cache::Cache};
use hmi_artifact::{asset::AssetObject, envelope};
use hmi_core::{Result, diagnostic::Diagnostic, error, report::Report, sha256};
use hmi_linker::{ExternalOutput, Linked};
use serde::{Deserialize, Serialize};

const MAGIC: &[u8; 8] = b"HMILNK01";
const SCHEMA: &str = "hmi.link-cache/1";
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Metadata {
    schema: String,
    key: String,
    hmi_length: usize,
    map: serde_json::Value,
    external: Vec<Member>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Member {
    path: String,
    length: usize,
}

fn encode(key: &str, linked: &Linked) -> Result<Vec<u8>> {
    let mut payload = linked.bytes.clone();
    let mut external = vec![];
    for output in &linked.external_files {
        external.push(Member {
            path: output.path.clone(),
            length: output.bytes.len(),
        });
        payload.extend_from_slice(&output.bytes);
    }
    envelope::encode(
        MAGIC,
        &Metadata {
            schema: SCHEMA.into(),
            key: key.into(),
            hmi_length: linked.bytes.len(),
            map: linked.map.clone(),
            external,
        },
        &payload,
        "link_cache",
    )
}
fn decode(key: &str, bytes: &[u8]) -> Result<Linked> {
    let (metadata, payload): (Metadata, Vec<u8>) = envelope::decode(MAGIC, bytes, "link_cache")?;
    if metadata.schema != SCHEMA
        || metadata.key != key
        || metadata.hmi_length == 0
        || metadata.map["schema"] != "hmi.link-map/1"
    {
        return Err(error("link_cache_metadata", "链接缓存描述无效"));
    }
    let hmi = payload
        .get(..metadata.hmi_length)
        .ok_or_else(|| error("link_cache_length", "链接缓存HMI长度越界"))?
        .to_vec();
    if metadata.map["output_sha256"] != sha256(&hmi) {
        return Err(error("link_cache_hash", "链接缓存HMI与映射不一致"));
    }
    let mut offset = metadata.hmi_length;
    let mut external_files = vec![];
    for member in metadata.external {
        hmi_core::deployment::validate(&member.path)?;
        let end = offset
            .checked_add(member.length)
            .ok_or_else(|| error("link_cache_length", "链接缓存成员长度溢出"))?;
        let bytes = payload
            .get(offset..end)
            .ok_or_else(|| error("link_cache_length", "链接缓存成员越界"))?
            .to_vec();
        external_files.push(ExternalOutput {
            path: member.path,
            bytes,
        });
        offset = end;
    }
    if offset != payload.len() {
        return Err(error("link_cache_length", "链接缓存含未声明载荷"));
    }
    Ok(Linked {
        bytes: hmi,
        diagnostics: vec![],
        map: metadata.map,
        inputs: vec![],
        external_files,
    })
}

pub fn link(
    prepared: &Prepared,
    assets: &[(String, AssetObject)],
    cache: &mut Cache,
    deny_warnings: bool,
    report: &Report,
) -> Result<Linked> {
    let task = report.task("link", Some("link"), "cache_check", "链接结果");
    let pages = prepared
        .pages
        .iter()
        .map(|p| p.object.clone())
        .collect::<Vec<_>>();
    // 顺序和当前资产符号都是链接输入；输出目录、任务并发数和告警策略不改变二进制。
    let input = serde_json::json!({
        "schema":SCHEMA,"target":prepared.target,
        "program":sha256(&prepared.program.object.encode()?),
        "pages":pages.iter().map(|p|p.encode().map(|b|sha256(&b))).collect::<Result<Vec<_>>>()?,
        "assets":assets.iter().map(|(key,asset)|asset.encode().map(|b|(key.clone(),sha256(&b)))).collect::<Result<Vec<_>>>()?,
        "original":prepared.original.as_ref().map(|o|o.encode().map(|b|sha256(&b))).transpose()?,
    });
    let key = cache.link_key(&input)?;
    if let Some(bytes) = cache.load_link()
        && let Ok(mut linked) = decode(&key, &bytes)
    {
        let checking = report.task("analysis", None, "check", "页面、脚本及资源引用");
        let infos = assets
            .iter()
            .map(|(key, asset)| (key.clone(), asset.metadata.info.clone()))
            .collect::<Vec<_>>();
        let diagnostics: Vec<Diagnostic> = hmi_linker::check_with_original(
            &prepared.target,
            &prepared.program.object,
            &pages,
            &infos,
            deny_warnings,
            prepared.original.as_ref(),
        )
        .map_err(|e| checking.error(e))?;
        checking.finish(false);
        linked.diagnostics = diagnostics;
        task.action("cache_hit");
        task.finish(true);
        return Ok(linked);
    }
    let linked = hmi_linker::link_with_original_and_report(
        &prepared.target,
        &prepared.program.object,
        &pages,
        assets,
        deny_warnings,
        prepared.original.as_ref(),
        report,
    )?;
    task.action("link");
    cache
        .store_link(&encode(&key, &linked)?)
        .map_err(|e| e.with_stage("cache"))?;
    task.finish(false);
    Ok(linked)
}
