import { useEffect, useState, type FormEvent } from "react";
import type { AppProps } from "@miy/sdk";
import { Button, Panel } from "@miy/components/ui";
import styles from "./AppRoot.module.css";

type Asset = { id: string; name: string; quantity: number; location: string | null };
const message = (error: unknown) => error instanceof Error ? error.message : "처리하지 못했습니다. 다시 시도해 주세요.";
export default function AppRoot({ runtime }: AppProps) {
  const [assets, setAssets] = useState<Asset[]>([]);
  const [name, setName] = useState("");
  const [quantity, setQuantity] = useState("0");
  const [location, setLocation] = useState("");
  const [editing, setEditing] = useState<Asset | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [result, setResult] = useState("");
  const dirty = editing ? name !== editing.name || quantity !== String(editing.quantity) || location !== (editing.location ?? "") : name !== "" || quantity !== "0" || location !== "";
  useEffect(() => runtime.setNavigationState({ dirty: dirty || busy, message: "저장되지 않은 작업이 있습니다. 이동하시겠습니까?" }), [runtime.setNavigationState, dirty, busy]);
  useEffect(() => {
    let active = true;
    const controller = new AbortController();
    runtime.api<{ assets: Asset[] }>("/assets", { signal: controller.signal })
      .then(data => { if (active) setAssets(data.assets); })
      .catch(err => { if (active) setError(message(err)); })
      .finally(() => { if (active) setLoading(false); });
    return () => { active = false; controller.abort(); };
  }, [runtime.api]);
  function reset() { setEditing(null); setName(""); setQuantity("0"); setLocation(""); }
  async function save(event: FormEvent) {
    event.preventDefault();
    if (busy) return;
    setError(""); setResult("");
    const count = Array.from(name.replace(/\s/gu, "")).length;
    if (count < 1 || count > 100 || !/^\d+$/.test(quantity) || Number(quantity) > 100000 || Array.from(location).length > 100) {
      setError("비품명은 공백 제외 1~100자, 수량은 0~100000의 정수로 입력해 주세요. 보관 위치는 최대 100자입니다."); return;
    }
    setBusy(true);
    try {
      const saved = await runtime.api<Asset>(editing ? `/assets/${editing.id}` : "/assets", {
        method: editing ? "PATCH" : "POST", headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ name: name.trim(), quantity: Number(quantity), location: location || null }),
      });
      setAssets(current => editing ? current.map(asset => asset.id === saved.id ? saved : asset) : [...current, saved]);
      setResult(editing ? "수정 내용을 저장했습니다." : "비품을 등록했습니다."); reset();
    } catch (err) { setError(message(err)); } finally { setBusy(false); }
  }
  async function remove(asset: Asset) {
    if (busy || !window.confirm(`‘${asset.name}’ 비품을 삭제하시겠습니까?`)) return;
    setBusy(true); setError(""); setResult("");
    try {
      await runtime.api(`/assets/${asset.id}`, { method: "DELETE" });
      setAssets(current => current.filter(item => item.id !== asset.id));
      if (editing?.id === asset.id) reset();
      setResult("비품을 삭제했습니다.");
    } catch (err) { setError(message(err)); } finally { setBusy(false); }
  }
  return <div className={styles.page}><Panel title="비품대장">
    <p className={styles.description}>내 비품의 수량과 현황을 관리하세요.</p>
    {error && <p role="alert" className={styles.error}>{error}</p>}
    {result && <p role="status" className={styles.success}>{result}</p>}
    <form onSubmit={save} className={styles.form}>
      <h2>{editing ? "비품 수정" : "비품 등록"}</h2>
      <fieldset disabled={busy || loading} className={styles.fields}>
        <label htmlFor="asset-name">비품명<input id="asset-name" value={name} onChange={event => setName(event.target.value)} required placeholder="예: 사무용 의자" /></label>
        <label htmlFor="asset-quantity">수량<input id="asset-quantity" type="number" min="0" max="100000" step="1" value={quantity} onChange={event => setQuantity(event.target.value)} required /></label>
        <label htmlFor="asset-location">보관 위치<input id="asset-location" value={location} onChange={event => setLocation(event.target.value)} placeholder="선택 · 최대 100자" /></label>
        <Button type="submit">{busy ? "처리 중…" : editing ? "저장" : "등록"}</Button>
        {(editing || dirty) && <Button type="button" onClick={reset}>취소</Button>}
      </fieldset>
    </form>
    <h2>내 비품 <span className={styles.count}>{assets.length}</span></h2>
    {loading ? <p role="status">목록을 불러오는 중…</p> : assets.length === 0 ? <p>등록된 비품이 없습니다.</p> :
      <div className={styles.tableWrap}><table className={styles.table}><thead><tr><th>비품명</th><th>수량</th><th>보관 위치</th><th>관리</th></tr></thead><tbody>
        {assets.map(asset => <tr key={asset.id}><td>{asset.name}</td><td>{asset.quantity.toLocaleString()}</td><td>{asset.location || "—"}</td><td className={styles.actions}>
          <Button type="button" disabled={busy || dirty || editing !== null} aria-label={`${asset.name} 수정`} onClick={() => { setEditing(asset); setName(asset.name); setQuantity(String(asset.quantity)); setLocation(asset.location ?? ""); setError(""); setResult(""); }}>수정</Button>
          <Button type="button" disabled={busy || dirty || editing !== null} aria-label={`${asset.name} 삭제`} onClick={() => void remove(asset)}>삭제</Button>
        </td></tr>)}
      </tbody></table></div>}
  </Panel></div>;
}
