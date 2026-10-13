// Điều khiển bản dev của MRM qua Chrome DevTools Protocol (WebView2, cổng 9229) — để thử tính năng không cần bấm tay.
// Chạy app dev trước (xem docs/HANDOVER.md, mục "Chạy thử an toàn").
//
//   node scripts/dev/cdp.mjs eval "<biểu thức JS>"       chạy trong trang, in kết quả (JSON). Có sẵn `I` = invoke của Tauri:
//                                                        node scripts/dev/cdp.mjs eval "I('list_libraries')"
//   node scripts/dev/cdp.mjs shot out.png [scale]        chụp cả cửa sổ (scale mặc định 1.25)
//   node scripts/dev/cdp.mjs file script.mjs             chạy file: export default async (I) => { ... } — tránh rắc rối dấu nháy / dấu \ của shell
import fs from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";

const [, , cmd, arg, extra] = process.argv;
const PORT = process.env.MRM_CDP_PORT || 9229;
let list;
try {
  list = await (await fetch(`http://127.0.0.1:${PORT}/json`)).json();
} catch {
  console.error(`Không kết nối được cổng ${PORT} — app dev đã chạy với scripts/dev/tauri.debug.json chưa?`);
  process.exit(1);
}
const page = list.find((p) => p.type === "page" && p.url.includes("localhost:1420")) ?? list.find((p) => p.type === "page");
const ws = new WebSocket(page.webSocketDebuggerUrl);
let id = 0;
const pending = new Map();
ws.onmessage = (m) => {
  const d = JSON.parse(m.data);
  if (d.id && pending.has(d.id)) {
    pending.get(d.id)(d);
    pending.delete(d.id);
  }
};
await new Promise((r) => (ws.onopen = r));
const send = (method, params = {}) =>
  new Promise((r) => {
    const i = ++id;
    pending.set(i, r);
    ws.send(JSON.stringify({ id: i, method, params }));
  });
const evaluate = async (body) => {
  const r = await send("Runtime.evaluate", {
    expression: `(async () => { const I = window.__TAURI_INTERNALS__.invoke; ${body} })()`,
    awaitPromise: true,
    returnByValue: true,
  });
  return r.result?.result?.value ?? r.result?.exceptionDetails?.exception?.description ?? r.result;
};

if (cmd === "eval") {
  console.log(JSON.stringify(await evaluate(`return ${arg}`), null, 1));
} else if (cmd === "file") {
  const fn = (await import(pathToFileURL(path.resolve(arg)).href)).default;
  console.log(JSON.stringify(await evaluate(`return (${fn.toString()})(I)`), null, 1));
} else if (cmd === "shot") {
  const scale = Number(extra || 1.25);
  const [w, h] = await evaluate("return [innerWidth, innerHeight]");
  const r = await send("Page.captureScreenshot", { format: "png", clip: { x: 0, y: 0, width: w, height: h, scale }, captureBeyondViewport: false });
  fs.writeFileSync(arg, Buffer.from(r.result.data, "base64"));
  console.log("saved", arg, w, h, scale);
} else {
  console.log("Dùng: eval <biểu thức> | file <script.mjs> | shot <out.png> [scale]");
}
ws.close();
process.exit(0);
