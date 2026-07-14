//! 評価用 HTML レポート生成。Python 版 report.py と同一の UI（採点・コメント・
//! JSON/CSV 入出力・全消去・生成物単位のキー）。

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;

use serde::Serialize;

#[derive(Serialize)]
pub struct Item {
    pub id: usize,
    pub name: String,
    pub src: String,
    pub out: Option<String>,
    pub status: String,
    pub confidence: f64,
    pub method: String,
    pub message: String,
    pub parts: serde_json::Value,
}

#[derive(Serialize)]
struct Data {
    items: Vec<Item>,
    project: String,
    gen: String,
}

pub fn write_report(items: Vec<Item>, out_dir: &Path) -> std::io::Result<std::path::PathBuf> {
    let mut hasher = DefaultHasher::new();
    for it in &items {
        format!("{}:{}:{}:{}", it.name, it.confidence, it.method, it.status).hash(&mut hasher);
    }
    let gen = format!("{:012x}", hasher.finish() & 0xffff_ffff_ffff);
    let data = Data {
        items,
        project: out_dir.canonicalize().unwrap_or_else(|_| out_dir.to_path_buf()).display().to_string(),
        gen,
    };
    let payload = serde_json::to_string(&data)
        .unwrap()
        .replace("</", "<\\/")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029");
    let html = TEMPLATE.replace("/*__DATA__*/", &payload);
    std::fs::create_dir_all(out_dir)?;
    let path = out_dir.join("report.html");
    std::fs::write(&path, html)?;
    Ok(path)
}

const TEMPLATE: &str = r####"<!doctype html>
<html lang="ja">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>slidewarp レビュー</title>
<style>
  :root { --bg:#f4f5f7; --card:#fff; --fg:#1c1e21; --muted:#6b7280; --line:#e3e6ea;
    --accent:#2563eb; --good:#16a34a; --warn:#d97706; --bad:#dc2626; }
  @media (prefers-color-scheme: dark) {
    :root { --bg:#15171b; --card:#1e2126; --fg:#e6e8eb; --muted:#9aa3ad; --line:#2c3038; --accent:#5b8def; }
  }
  * { box-sizing:border-box; }
  body { margin:0; font-family:system-ui,-apple-system,"Segoe UI",Roboto,"Noto Sans JP",sans-serif;
         background:var(--bg); color:var(--fg); }
  header { position:sticky; top:0; z-index:10; background:var(--card); border-bottom:1px solid var(--line);
           padding:12px 20px; display:flex; gap:16px; align-items:center; flex-wrap:wrap; }
  header h1 { font-size:16px; margin:0; font-weight:700; }
  .sp { flex:1; }
  .stat { font-size:13px; color:var(--muted); } .stat b { color:var(--fg); }
  button { font:inherit; padding:7px 12px; border:1px solid var(--line); background:var(--card); color:var(--fg);
           border-radius:8px; cursor:pointer; }
  button:hover { border-color:var(--accent); }
  button.primary { background:var(--accent); color:#fff; border-color:var(--accent); }
  button.danger { color:var(--bad); border-color:var(--bad); }
  button.danger:hover { background:var(--bad); color:#fff; }
  main { padding:16px 20px 80px; display:flex; flex-direction:column; gap:16px; max-width:1400px; margin:0 auto; }
  .card { background:var(--card); border:1px solid var(--line); border-radius:12px; overflow:hidden; }
  .card-head { display:flex; align-items:center; gap:10px; padding:10px 14px; border-bottom:1px solid var(--line); flex-wrap:wrap; }
  .idx { font-variant-numeric:tabular-nums; color:var(--muted); font-size:13px; }
  .name { font-weight:600; font-size:14px; word-break:break-all; }
  .badge { font-size:12px; padding:2px 8px; border-radius:999px; border:1px solid var(--line); color:var(--muted); }
  .badge.ok { color:var(--good); border-color:var(--good); }
  .badge.low_confidence, .badge.no_detection { color:var(--warn); border-color:var(--warn); }
  .badge.error { color:var(--bad); border-color:var(--bad); }
  .body { display:grid; grid-template-columns: 1fr 1fr 320px; gap:0; }
  @media (max-width: 1000px){ .body { grid-template-columns:1fr; } }
  .imgcol { padding:12px; border-right:1px solid var(--line); }
  .imgcol h3 { margin:0 0 8px; font-size:12px; color:var(--muted); font-weight:600; }
  .imgwrap { background:repeating-conic-gradient(#0000 0 25%, #8883 0 50%) 0 0/16px 16px; border-radius:8px; overflow:hidden; }
  .imgwrap img { display:block; width:100%; height:auto; max-height:420px; object-fit:contain; cursor:zoom-in; }
  .eval { padding:14px; display:flex; flex-direction:column; gap:14px; }
  .meta { font-size:12px; color:var(--muted); line-height:1.7; } .meta code { color:var(--fg); }
  .rate label { display:block; font-size:12px; font-weight:600; margin-bottom:6px; }
  .stars { display:flex; gap:4px; }
  .stars button { width:34px; padding:6px 0; text-align:center; }
  .stars button.on { background:var(--accent); color:#fff; border-color:var(--accent); }
  .stars .na { width:auto; padding:6px 8px; font-size:12px; }
  textarea { width:100%; min-height:64px; resize:vertical; padding:8px; border:1px solid var(--line);
             border-radius:8px; background:var(--bg); color:var(--fg); font:inherit; }
  .saved { font-size:11px; color:var(--good); opacity:0; transition:opacity .3s; } .saved.show { opacity:1; }
  dialog { border:none; border-radius:12px; padding:0; background:transparent; max-width:96vw; max-height:96vh; }
  dialog img { max-width:96vw; max-height:96vh; border-radius:12px; }
  dialog::backdrop { background:rgba(0,0,0,.85); }
  select { font:inherit; padding:6px 8px; border:1px solid var(--line); border-radius:8px; background:var(--card); color:var(--fg); }
</style>
</head>
<body>
<header>
  <h1>slidewarp レビュー</h1>
  <div class="filterbar"><label>表示:
    <select id="filter">
      <option value="all">すべて</option>
      <option value="unrated">未評価のみ</option>
      <option value="rated">評価済みのみ</option>
      <option value="low">低信頼/未検出/エラー</option>
    </select></label></div>
  <span class="sp"></span>
  <span class="stat" id="progress"></span>
  <span class="stat" id="avg"></span>
  <button id="import">JSON取込</button>
  <button id="csv">CSV出力</button>
  <button id="export" class="primary">JSON出力</button>
  <button id="clear" class="danger">全消去</button>
  <input type="file" id="importfile" accept="application/json" hidden>
</header>
<main id="list"></main>
<dialog id="zoom"><img id="zoomimg" alt=""></dialog>
<script>
const DATA = /*__DATA__*/;
const KEY = "slidewarp-eval:" + DATA.project + ":" + (DATA.gen || "");
const RATE_FIELDS = [
  {key:"crop", label:"切り出し位置（幾何補正）"},
  {key:"look", label:"見た目（色調/露出/シャープ）"},
];
let store = {};
try { store = JSON.parse(localStorage.getItem(KEY) || "{}"); } catch(e) { store = {}; }
let lsWarned=false;
function saveStore(){ try{ localStorage.setItem(KEY, JSON.stringify(store)); }catch(e){ if(!lsWarned){lsWarned=true;console.warn("localStorage不可。JSON出力で保存を。",e);} } }
function rec(id){ return store[id] || (store[id]={crop:null,look:null,comment:""}); }
function esc(s){ return String(s==null?"":s).replace(/[&<>"]/g,c=>({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;"}[c])); }
function fmtParts(p){ if(!p) return ""; return Object.entries(p).map(([k,v])=>`${k}=${typeof v==="number"?(+v).toFixed(3).replace(/\.?0+$/,""):v}`).join(", "); }
function render(){
  const filter=document.getElementById("filter").value; const list=document.getElementById("list"); list.innerHTML="";
  let shown=0;
  for(const it of DATA.items){
    const r=rec(it.id); const rated=r.crop!=null||r.look!=null||(r.comment||"").trim()!==""; const low=it.status!=="ok";
    if(filter==="unrated"&&rated) continue; if(filter==="rated"&&!rated) continue; if(filter==="low"&&!low) continue;
    shown++; list.appendChild(makeCard(it,r));
  }
  if(shown===0) list.innerHTML='<p class="stat">該当する画像はありません。</p>';
  updateStats();
}
function makeCard(it,r){
  const card=document.createElement("div"); card.className="card";
  const outImg=it.out?`<div class="imgwrap"><img loading="lazy" src="${it.out}" alt=""></div>`:`<p class="meta">出力なし（${it.status}）</p>`;
  card.innerHTML=`
    <div class="card-head">
      <span class="idx">#${String(it.id+1).padStart(2,"0")}</span>
      <span class="name">${esc(it.name)}</span>
      <span class="badge ${it.status}">${it.status}</span>
      <span class="badge">${esc(it.method||"-")} / conf ${it.confidence}</span>
      <span class="sp"></span><span class="saved" data-saved="${it.id}">保存しました</span>
    </div>
    <div class="body">
      <div class="imgcol"><h3>元画像</h3><div class="imgwrap"><img loading="lazy" src="${it.src}" alt=""></div></div>
      <div class="imgcol"><h3>処理後</h3>${outImg}</div>
      <div class="eval">
        <div class="meta">手法: <code>${esc(it.method||"-")}</code> / 信頼度: <code>${it.confidence}</code>${it.message?`<br>備考: ${esc(it.message)}`:""}${it.parts&&Object.keys(it.parts).length?`<br><span style="font-size:11px">${fmtParts(it.parts)}</span>`:""}</div>
        ${RATE_FIELDS.map(f=>rateHtml(it.id,f,r[f.key])).join("")}
        <div><label style="font-size:12px;font-weight:600;display:block;margin-bottom:6px">改善点コメント</label>
        <textarea data-comment="${it.id}" placeholder="例: 上端がクリップ / 色が青い / 傾き残り など">${esc(r.comment||"")}</textarea></div>
      </div>
    </div>`;
  card.querySelectorAll(".imgwrap img").forEach(img=>img.addEventListener("click",()=>{const z=document.getElementById("zoom");document.getElementById("zoomimg").src=img.src;z.showModal();}));
  card.querySelectorAll("[data-rate]").forEach(btn=>btn.addEventListener("click",()=>{
    const [id,field,val]=btn.dataset.rate.split("|"); const rr=rec(id);
    const nv=(val==="na")?"na":+val; rr[field]=(rr[field]===nv)?null:nv; saveStore(); flash(id);
    btn.parentElement.querySelectorAll("button").forEach(b=>{const v=b.dataset.rate.split("|")[2];b.classList.toggle("on",rr[field]!=null&&String(rr[field])===v);});
    updateStats();
  }));
  const ta=card.querySelector("[data-comment]");
  ta.addEventListener("input",()=>{rec(it.id).comment=ta.value;});
  ta.addEventListener("change",()=>{saveStore();flash(it.id);updateStats();});
  return card;
}
function rateHtml(id,f,cur){
  let btns=""; for(let v=1;v<=5;v++) btns+=`<button data-rate="${id}|${f.key}|${v}" class="${String(cur)===String(v)?"on":""}">${v}</button>`;
  btns+=`<button class="na ${cur==="na"?"on":""}" data-rate="${id}|${f.key}|na">対象外</button>`;
  return `<div class="rate"><label>${f.label}<span style="font-weight:400;color:var(--muted)"> （1=悪い〜5=良い）</span></label><div class="stars">${btns}</div></div>`;
}
function flash(id){const el=document.querySelector(`[data-saved="${id}"]`);if(!el)return;el.classList.add("show");setTimeout(()=>el.classList.remove("show"),900);}
function updateStats(){
  const n=DATA.items.length; let rated=0,cs=0,cn=0,ls=0,ln=0;
  for(const it of DATA.items){const r=store[it.id];if(!r)continue;
    if(r.crop!=null||r.look!=null||(r.comment||"").trim()!=="")rated++;
    if(typeof r.crop==="number"){cs+=r.crop;cn++;} if(typeof r.look==="number"){ls+=r.look;ln++;}}
  document.getElementById("progress").innerHTML=`評価 <b>${rated}</b>/${n}`;
  document.getElementById("avg").innerHTML=`平均 切り出し <b>${cn?(cs/cn).toFixed(2):"-"}</b> / 見た目 <b>${ln?(ls/ln).toFixed(2):"-"}</b>`;
}
function download(name,text,type){const b=new Blob([text],{type});const u=URL.createObjectURL(b);const a=document.createElement("a");a.href=u;a.download=name;a.click();URL.revokeObjectURL(u);}
function exportRows(){return DATA.items.map(it=>{const r=store[it.id]||{};return {name:it.name,status:it.status,method:it.method,confidence:it.confidence,crop:r.crop??"",look:r.look??"",comment:(r.comment||"").replace(/\r?\n/g," ")};});}
document.getElementById("export").onclick=()=>download("slidewarp-eval.json",JSON.stringify(exportRows(),null,2),"application/json");
document.getElementById("csv").onclick=()=>{const rows=exportRows();const head=["name","status","method","confidence","crop","look","comment"];const e=s=>`"${String(s).replace(/"/g,'""')}"`;const csv=[head.join(",")].concat(rows.map(r=>head.map(h=>e(r[h])).join(","))).join("\n");download("slidewarp-eval.csv","﻿"+csv,"text/csv");};
document.getElementById("import").onclick=()=>document.getElementById("importfile").click();
document.getElementById("importfile").onchange=(ev)=>{const f=ev.target.files[0];if(!f)return;const fr=new FileReader();fr.onload=()=>{try{const rows=JSON.parse(fr.result);const by={};DATA.items.forEach(it=>by[it.name]=it.id);for(const row of rows){const id=by[row.name];if(id==null)continue;store[id]={crop:row.crop===""?null:row.crop,look:row.look===""?null:row.look,comment:row.comment||""};}saveStore();render();}catch(err){alert("取込失敗: "+err);}};fr.readAsText(f);};
document.getElementById("clear").onclick=()=>{const n=Object.keys(store).length;if(!confirm(`入力済みの評価(${n}件)をすべて消去します。よろしいですか？`))return;store={};try{localStorage.removeItem(KEY);}catch(e){}render();};
document.getElementById("filter").onchange=render;
document.getElementById("zoom").addEventListener("click",(e)=>{if(e.target.id==="zoom")e.target.close();});
render();
</script>
</body>
</html>
"####;
