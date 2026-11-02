import { type FormEvent, useCallback, useEffect, useState } from "react";
import type { AppProps } from "@miy/sdk";
import { Button, Panel } from "@miy/components/ui";
import styles from "./AppRoot.module.css";
import { type Asset, errorMessage, parseAssetInput } from "./assetForm";

type Notice = { kind: "status" | "error"; text: string } | null;
type Editing = { id: string; name: string; quantity: string; location: string } | null;

const JSON_HEADERS = { "Content-Type": "application/json" };
// Allows surrounding whitespace while still preventing unbounded input.
const NAME_INPUT_MAX = 200;
const LOCATION_INPUT_MAX = 200;

export default function AppRoot({ runtime }: AppProps) {
  const [assets, setAssets] = useState<Asset[]>([]);
  const [loading, setLoading] = useState(true);
  const [name, setName] = useState("");
  const [quantity, setQuantity] = useState("");
  const [location, setLocation] = useState("");
  const [editing, setEditing] = useState<Editing>(null);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<Notice>(null);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const data = await runtime.api<{ assets: Asset[] }>("/assets");
      setAssets(data.assets);
    } catch (e) {
      setNotice({ kind: "error", text: `목록을 불러오지 못했습니다: ${errorMessage(e)}` });
    } finally {
      setLoading(false);
    }
  }, [runtime.api]);

  useEffect(() => {
    void load();
  }, [load]);

  const editingAsset = editing ? assets.find((a) => a.id === editing.id) : undefined;
  const editDirty =
    !!editing &&
    !!editingAsset &&
    (editing.name !== editingAsset.name ||
      editing.quantity !== String(editingAsset.quantity) ||
      editing.location !== (editingAsset.location ?? ""));
  const dirty = name !== "" || quantity !== "" || location !== "" || editDirty;

  useEffect(
    () =>
      runtime.registerNavigationGuard(() =>
        dirty || busy ? "저장되지 않은 작업이 있습니다. 이동하시겠습니까?" : undefined,
      ),
    [runtime.registerNavigationGuard, dirty, busy],
  );

  async function run(action: () => Promise<string>) {
    setBusy(true);
    setNotice(null);
    try {
      setNotice({ kind: "status", text: await action() });
    } catch (e) {
      setNotice({ kind: "error", text: errorMessage(e) });
    } finally {
      setBusy(false);
    }
  }

  function create(event: FormEvent) {
    event.preventDefault();
    const parsed = parseAssetInput(name, quantity, location);
    if (!parsed.ok) {
      setNotice({ kind: "error", text: parsed.message });
      return;
    }
    void run(async () => {
      const created = await runtime.api<Asset>("/assets", {
        method: "POST",
        headers: JSON_HEADERS,
        body: JSON.stringify(parsed.value),
      });
      setAssets((list) => [...list, created]);
      setName("");
      setQuantity("");
      setLocation("");
      return `'${created.name}' 비품을 등록했습니다.`;
    });
  }

  function save(event: FormEvent) {
    event.preventDefault();
    if (!editing || !editingAsset) return;
    const parsed = parseAssetInput(editing.name, editing.quantity, editing.location);
    if (!parsed.ok) {
      setNotice({ kind: "error", text: parsed.message });
      return;
    }
    const changes: Partial<Pick<Asset, "name" | "quantity" | "location">> = {};
    if (parsed.value.name !== editingAsset.name) changes.name = parsed.value.name;
    if (parsed.value.quantity !== editingAsset.quantity) changes.quantity = parsed.value.quantity;
    if (parsed.value.location !== (editingAsset.location ?? null))
      changes.location = parsed.value.location;
    if (Object.keys(changes).length === 0) {
      setEditing(null);
      setNotice({ kind: "status", text: "변경된 내용이 없습니다." });
      return;
    }
    void run(async () => {
      const updated = await runtime.api<Asset>(`/assets/${encodeURIComponent(editing.id)}`, {
        method: "PATCH",
        headers: JSON_HEADERS,
        body: JSON.stringify(changes),
      });
      setAssets((list) => list.map((a) => (a.id === updated.id ? updated : a)));
      setEditing(null);
      return `'${updated.name}' 비품을 저장했습니다.`;
    });
  }

  function remove(asset: Asset) {
    void run(async () => {
      await runtime.api<{ id: string }>(`/assets/${encodeURIComponent(asset.id)}`, {
        method: "DELETE",
      });
      setAssets((list) => list.filter((a) => a.id !== asset.id));
      if (editing?.id === asset.id) setEditing(null);
      return `'${asset.name}' 비품을 삭제했습니다.`;
    });
  }

  return (
    <Panel title="비품대장">
      <form className={styles.form} onSubmit={create} aria-label="비품 등록">
        <label className={styles.field}>
          비품명
          <input
            className={styles.nameInput}
            value={name}
            maxLength={NAME_INPUT_MAX}
            onChange={(e) => setName(e.target.value)}
            disabled={busy}
          />
        </label>
        <label className={styles.field}>
          수량
          <input
            className={styles.quantityInput}
            type="number"
            inputMode="numeric"
            min={0}
            max={100000}
            step={1}
            value={quantity}
            onChange={(e) => setQuantity(e.target.value)}
            disabled={busy}
          />
        </label>
        <label className={styles.field}>
          보관 위치
          <input
            className={styles.nameInput}
            value={location}
            maxLength={LOCATION_INPUT_MAX}
            placeholder="선택 입력"
            onChange={(e) => setLocation(e.target.value)}
            disabled={busy}
          />
        </label>
        <Button type="submit" disabled={busy}>
          등록
        </Button>
      </form>

      {notice && (
        <p
          className={notice.kind === "error" ? styles.error : styles.status}
          role={notice.kind === "error" ? "alert" : "status"}
        >
          {notice.text}
        </p>
      )}

      {loading ? (
        <p className={styles.empty}>불러오는 중…</p>
      ) : assets.length === 0 ? (
        <p className={styles.empty}>등록된 비품이 없습니다.</p>
      ) : (
        <table className={styles.table}>
          <thead>
            <tr>
              <th>비품명</th>
              <th className={styles.number}>수량</th>
              <th>보관 위치</th>
              <th>작업</th>
            </tr>
          </thead>
          <tbody>
            {assets.map((asset) =>
              editing?.id === asset.id ? (
                <tr key={asset.id}>
                  <td>
                    <input
                      className={styles.nameInput}
                      aria-label="비품명 수정"
                      value={editing.name}
                      maxLength={NAME_INPUT_MAX}
                      onChange={(e) => setEditing({ ...editing, name: e.target.value })}
                      disabled={busy}
                      form={`edit-${asset.id}`}
                    />
                  </td>
                  <td className={styles.number}>
                    <input
                      className={styles.quantityInput}
                      aria-label="수량 수정"
                      type="number"
                      inputMode="numeric"
                      min={0}
                      max={100000}
                      step={1}
                      value={editing.quantity}
                      onChange={(e) => setEditing({ ...editing, quantity: e.target.value })}
                      disabled={busy}
                      form={`edit-${asset.id}`}
                    />
                  </td>
                  <td>
                    <input
                      className={styles.nameInput}
                      aria-label="보관 위치 수정"
                      value={editing.location}
                      maxLength={LOCATION_INPUT_MAX}
                      onChange={(e) => setEditing({ ...editing, location: e.target.value })}
                      disabled={busy}
                      form={`edit-${asset.id}`}
                    />
                  </td>
                  <td>
                    <form id={`edit-${asset.id}`} className={styles.actions} onSubmit={save}>
                      <Button type="submit" disabled={busy}>
                        저장
                      </Button>
                      <Button type="button" disabled={busy} onClick={() => setEditing(null)}>
                        취소
                      </Button>
                    </form>
                  </td>
                </tr>
              ) : (
                <tr key={asset.id}>
                  <td>{asset.name}</td>
                  <td className={styles.number}>{asset.quantity.toLocaleString("ko-KR")}</td>
                  <td>{asset.location ? asset.location : <span className={styles.empty}>—</span>}</td>
                  <td>
                    <div className={styles.actions}>
                      <Button
                        type="button"
                        disabled={busy || editDirty}
                        onClick={() =>
                          setEditing({
                            id: asset.id,
                            name: asset.name,
                            quantity: String(asset.quantity),
                            location: asset.location ?? "",
                          })
                        }
                      >
                        수정
                      </Button>
                      <Button type="button" disabled={busy} onClick={() => remove(asset)}>
                        삭제
                      </Button>
                    </div>
                  </td>
                </tr>
              ),
            )}
          </tbody>
        </table>
      )}
    </Panel>
  );
}

