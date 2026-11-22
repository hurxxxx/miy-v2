import type { paths } from '../../api.generated';
import { apiFetchJson } from '../api/client';
import type { AppApi } from '../runtime/runtime-context';

const todosPath = '/v1/personal-widgets/todos';
const todoPath = '/v1/personal-widgets/todos/{todo_id}';
const memoPath = '/v1/personal-widgets/memo';
type Todos = paths[typeof todosPath];
type Todo = paths[typeof todoPath];
type MemoOperations = paths[typeof memoPath];
type TodosQuery = NonNullable<Todos['get']['parameters']['query']>;
type CreateTodoInput = Todos['post']['requestBody']['content']['application/json'];
type UpdateTodoInput = Todo['patch']['requestBody']['content']['application/json'];
export type PersonalTodoItem = Todos['post']['responses'][201]['content']['application/json'];
export type PersonalTodoListResponse = Todos['get']['responses'][200]['content']['application/json'];
export type PersonalMemo = MemoOperations['get']['responses'][200]['content']['application/json'];
type SaveMemoInput = MemoOperations['put']['requestBody']['content']['application/json'];

function personalTodoPath({ todo_id }: Todo['patch']['parameters']['path']): string {
  return todoPath.replace('{todo_id}', encodeURIComponent(todo_id));
}

// miy: `api` (runtime.api) replaces AI-DO's bearer token; paths are app
// relative and reach /api/v1/personal-widgets/... as before.
export function listPersonalTodos(
  api: AppApi,
  includeCompleted = true,
): Promise<PersonalTodoListResponse> {
  const options = {
    include_completed: includeCompleted ? 'true' : 'false',
  } satisfies TodosQuery;
  const query = new URLSearchParams(options);
  return apiFetchJson<PersonalTodoListResponse>(
    `${todosPath}?${query.toString()}`,
    api,
  );
}

export function createPersonalTodo(
  api: AppApi,
  title: string,
): Promise<PersonalTodoItem> {
  const input = { title } satisfies CreateTodoInput;
  return apiFetchJson<PersonalTodoItem>(
    todosPath,
    api,
    {
      body: JSON.stringify(input),
      method: 'POST',
    },
  );
}

export function updatePersonalTodo(
  api: AppApi,
  todoId: string,
  payload: UpdateTodoInput,
): Promise<PersonalTodoItem> {
  return apiFetchJson<PersonalTodoItem>(
    personalTodoPath({ todo_id: todoId }),
    api,
    {
      body: JSON.stringify(payload),
      method: 'PATCH',
    },
  );
}

export function deletePersonalTodo(api: AppApi, todoId: string): Promise<void> {
  return apiFetchJson<void>(
    personalTodoPath({ todo_id: todoId }),
    api,
    { method: 'DELETE' },
  );
}

export function getPersonalMemo(api: AppApi): Promise<PersonalMemo> {
  return apiFetchJson<PersonalMemo>(memoPath, api);
}

export function savePersonalMemo(
  api: AppApi,
  body: string,
  expectedUpdatedAt: string | null,
): Promise<PersonalMemo> {
  const input: SaveMemoInput = { body, expectedUpdatedAt };
  return apiFetchJson<PersonalMemo>(
    memoPath,
    api,
    {
      body: JSON.stringify(input),
      method: 'PUT',
    },
  );
}
