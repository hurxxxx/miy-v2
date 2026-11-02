export interface Asset {
  id: string;
  name: string;
  quantity: number;
  /** null for records created before the location column existed. */
  location: string | null;
}

export const NAME_MAX = 100;
export const QUANTITY_MAX = 100000;
export const LOCATION_MAX = 100;

export type Parsed =
  | { ok: true; value: { name: string; quantity: number; location: string | null } }
  | { ok: false; message: string };

/** Mirrors the server rules so users get immediate feedback; the server stays authoritative. */
export function parseAssetInput(name: string, quantity: string, location = ""): Parsed {
  const trimmed = name.trim();
  if (!trimmed) return { ok: false, message: "비품명을 입력하세요." };
  if ([...trimmed].length > NAME_MAX)
    return { ok: false, message: `비품명은 ${NAME_MAX}자 이하여야 합니다.` };
  const q = quantity.trim();
  if (!/^\d+$/.test(q) || Number(q) > QUANTITY_MAX)
    return {
      ok: false,
      message: `수량은 0 이상 ${QUANTITY_MAX} 이하의 정수여야 합니다.`,
    };
  const place = location.trim();
  if ([...place].length > LOCATION_MAX)
    return { ok: false, message: `보관 위치는 ${LOCATION_MAX}자 이하여야 합니다.` };
  return { ok: true, value: { name: trimmed, quantity: Number(q), location: place || null } };
}

const ERROR_MESSAGES: Record<string, string> = {
  invalid_name: "비품명이 올바르지 않습니다. 공백을 제외하고 1~100자로 입력하세요.",
  invalid_quantity: "수량은 0 이상 100000 이하의 정수여야 합니다.",
  invalid_json: "요청 형식이 올바르지 않습니다.",
  invalid_body: "요청 형식이 올바르지 않습니다.",
  unknown_field: "허용되지 않는 값이 포함되어 있습니다.",
  invalid_location: "보관 위치는 100자 이하의 문자열이어야 합니다.",
  empty_patch: "수정할 내용이 없습니다.",
  invalid_id: "비품 ID가 올바르지 않습니다.",
  asset_not_found: "비품을 찾을 수 없습니다. 목록을 새로고침하세요.",
  database_error: "데이터를 처리하지 못했습니다. 잠시 후 다시 시도하세요.",
};

export function errorMessage(error: unknown): string {
  const raw = error instanceof Error ? error.message : String(error);
  return ERROR_MESSAGES[raw] ?? `요청을 처리하지 못했습니다. (${raw})`;
}
