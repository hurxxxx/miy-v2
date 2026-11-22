import { apiFetchJson } from '../api/client';
import type { AppApi } from '../runtime/runtime-context';

// AI-DO derived these from its generated OpenAPI types
// (ApiSchema<'PersonalTodoItemOut'> ...); the same wire shapes, written out.
export interface PersonalTodoItem {
  id: string;
  title: string;
  completed: boolean;
  sortOrder: number;
  completedAt: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface PersonalTodoListResponse {
  items: PersonalTodoItem[];
}

export interface PersonalMemo {
  id: string | null;
  body: string;
  createdAt: string | null;
  updatedAt: string | null;
}

// miy: `api` (runtime.api) replaces AI-DO's bearer token; paths are app
// relative and reach /api/v1/personal-widgets/... as before.
export function listPersonalTodos(
  api: AppApi,
  includeCompleted = true,
): Promise<PersonalTodoListResponse> {
  const query = new URLSearchParams({
    include_completed: includeCompleted ? 'true' : 'false',
  });
  return apiFetchJson<PersonalTodoListResponse>(
    `/v1/personal-widgets/todos?${query.toString()}`,
    api,
  );
}

export function createPersonalTodo(
  api: AppApi,
  title: string,
): Promise<PersonalTodoItem> {
  return apiFetchJson<PersonalTodoItem>(
    '/v1/personal-widgets/todos',
    api,
    {
      body: JSON.stringify({ title }),
      method: 'POST',
    },
  );
}

export function updatePersonalTodo(
  api: AppApi,
  todoId: string,
  payload: { completed?: boolean; title?: string },
): Promise<PersonalTodoItem> {
  return apiFetchJson<PersonalTodoItem>(
    `/v1/personal-widgets/todos/${encodeURIComponent(todoId)}`,
    api,
    {
      body: JSON.stringify(payload),
      method: 'PATCH',
    },
  );
}

export function deletePersonalTodo(api: AppApi, todoId: string): Promise<void> {
  return apiFetchJson<void>(
    `/v1/personal-widgets/todos/${encodeURIComponent(todoId)}`,
    api,
    { method: 'DELETE' },
  );
}

export function getPersonalMemo(api: AppApi): Promise<PersonalMemo> {
  return apiFetchJson<PersonalMemo>('/v1/personal-widgets/memo', api);
}

export function savePersonalMemo(
  api: AppApi,
  body: string,
): Promise<PersonalMemo> {
  return apiFetchJson<PersonalMemo>(
    '/v1/personal-widgets/memo',
    api,
    {
      body: JSON.stringify({ body }),
      method: 'PUT',
    },
  );
}
