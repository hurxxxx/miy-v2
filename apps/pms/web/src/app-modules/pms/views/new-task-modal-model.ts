import type { BlockContent } from '../../../components/editor';

import type {
  createTaskListTask,
  PmsTaskListStatus,
  PmsTaskTemplate,
} from '../api/pms-api';
import { getDefaultTaskStatus } from './pms-constants';

export type NewTaskCreatePayload = Parameters<typeof createTaskListTask>[2];

export type NewTaskState = {
  title: string;
  parentId: string | null;
  status: string;
  priority: string;
  startDate: string;
  dueDate: string;
  assignToMe: boolean;
  showDescription: boolean;
  descriptionBlocks: BlockContent | undefined;
  submitting: boolean;
  templates: PmsTaskTemplate[];
  templateMenuOpen: boolean;
  /** miy: the template the task is made from; its checklist, labels and
   *  subtasks are added by the server when the task is created. */
  appliedTemplate: PmsTaskTemplate | null;
  /** Remounts the description editor when a template replaces its text. */
  descriptionVersion: number;
  error: string | null;
};

export type NewTaskAction =
  | {
      type: 'title';
      value: string;
    }
  | {
      type: 'priority';
      value: string;
    }
  | {
      type: 'parent-id';
      value: string | null;
    }
  | {
      type: 'start-date';
      value: string;
    }
  | {
      type: 'due-date';
      value: string;
    }
  | {
      type: 'assign-to-me';
      value: boolean;
    }
  | {
      type: 'show-description';
    }
  | {
      type: 'description-blocks';
      value: BlockContent | undefined;
    }
  | {
      type: 'open-template-menu';
    }
  | {
      type: 'close-template-menu';
    }
  | {
      type: 'templates-loaded';
      templates: PmsTaskTemplate[];
    }
  | {
      type: 'apply-template';
      taskListStatuses?: PmsTaskListStatus[];
      template: PmsTaskTemplate;
    }
  | {
      type: 'clear-template';
    }
  | {
      type: 'submit';
    }
  | {
      type: 'failed';
      message: string;
    }
  | {
      type: 'finished';
    };

export function createInitialNewTaskState(
  taskListStatuses?: PmsTaskListStatus[],
  initialTitle = '',
): NewTaskState {
  return {
    title: initialTitle.trim(),
    parentId: null,
    status: getDefaultTaskStatus(taskListStatuses),
    priority: 'medium',
    startDate: '',
    dueDate: '',
    assignToMe: false,
    showDescription: false,
    descriptionBlocks: undefined,
    submitting: false,
    templates: [],
    templateMenuOpen: false,
    appliedTemplate: null,
    descriptionVersion: 0,
    error: null,
  };
}

export function reconcileNewTaskStartDate(
  startDate: string,
  dueDate: string,
): Pick<NewTaskState, 'startDate' | 'dueDate'> {
  return {
    startDate,
    dueDate: startDate && dueDate && startDate > dueDate ? startDate : dueDate,
  };
}

export function reconcileNewTaskDueDate(
  startDate: string,
  dueDate: string,
): Pick<NewTaskState, 'startDate' | 'dueDate'> {
  return {
    startDate:
      dueDate && startDate && dueDate < startDate ? dueDate : startDate,
    dueDate,
  };
}

export function resolveNewTaskStatus(
  status: string,
  taskListStatuses?: PmsTaskListStatus[],
): string {
  const hasInvalidTaskListStatus =
    taskListStatuses !== undefined &&
    taskListStatuses.length > 0 &&
    !taskListStatuses.some((taskListStatus) => taskListStatus.slug === status);
  if (status === 'backlog' || hasInvalidTaskListStatus) {
    return getDefaultTaskStatus(taskListStatuses);
  }
  return status;
}

export function applyNewTaskTemplate(
  state: NewTaskState,
  template: PmsTaskTemplate,
  taskListStatuses?: PmsTaskListStatus[],
): NewTaskState {
  // miy: the template's own task title when it has one (AI-DO used the
  // template name), and its formatted description.
  const blocks = template.description_blocks?.length
    ? (template.description_blocks as unknown as BlockContent)
    : undefined;
  return {
    ...state,
    title: template.title?.trim() || template.name,
    status: resolveNewTaskStatus(template.default_status, taskListStatuses),
    priority: template.default_priority,
    showDescription:
      state.showDescription || Boolean(template.description) || Boolean(blocks),
    descriptionBlocks: blocks ?? state.descriptionBlocks,
    descriptionVersion: blocks
      ? state.descriptionVersion + 1
      : state.descriptionVersion,
    appliedTemplate: template,
    templateMenuOpen: false,
  };
}

export function buildNewTaskCreatePayload(
  state: NewTaskState,
  taskListStatuses?: PmsTaskListStatus[],
  options: { assignToUserId?: string | null } = {},
): NewTaskCreatePayload {
  return {
    title: state.title.trim(),
    description: '',
    description_blocks: state.descriptionBlocks ?? null,
    status: resolveNewTaskStatus(state.status, taskListStatuses),
    priority: state.priority,
    assignee_id: state.assignToMe ? (options.assignToUserId ?? null) : null,
    milestone_id: null,
    parent_id: state.parentId,
    start_date: state.startDate || null,
    due_date: state.dueDate || null,
    template_id: state.appliedTemplate?.id ?? null,
  };
}

export function newTaskReducer(
  state: NewTaskState,
  action: NewTaskAction,
): NewTaskState {
  switch (action.type) {
    case 'title':
      return {
        ...state,
        title: action.value,
      };
    case 'priority':
      return {
        ...state,
        priority: action.value,
      };
    case 'parent-id':
      return {
        ...state,
        parentId: action.value,
      };
    case 'start-date':
      return {
        ...state,
        ...reconcileNewTaskStartDate(action.value, state.dueDate),
      };
    case 'due-date':
      return {
        ...state,
        ...reconcileNewTaskDueDate(state.startDate, action.value),
      };
    case 'assign-to-me':
      return {
        ...state,
        assignToMe: action.value,
      };
    case 'show-description':
      return {
        ...state,
        showDescription: true,
      };
    case 'description-blocks':
      return {
        ...state,
        descriptionBlocks: action.value,
      };
    case 'open-template-menu':
      return {
        ...state,
        templateMenuOpen: true,
      };
    case 'close-template-menu':
      return {
        ...state,
        templateMenuOpen: false,
      };
    case 'templates-loaded':
      return {
        ...state,
        templates: action.templates,
      };
    case 'apply-template':
      return applyNewTaskTemplate(
        state,
        action.template,
        action.taskListStatuses,
      );
    case 'clear-template':
      return {
        ...state,
        appliedTemplate: null,
      };
    case 'submit':
      return {
        ...state,
        submitting: true,
        error: null,
      };
    case 'failed':
      return {
        ...state,
        error: action.message,
      };
    case 'finished':
      return {
        ...state,
        submitting: false,
      };
  }
}

/**
 * miy: whether the new-task form holds input worth asking about before the
 * person leaves the app (reported with `runtime.setNavigationState`). The
 * default status and an untouched description do not count; a create in
 * flight does.
 */
export function isNewTaskDraftDirty(
  state: NewTaskState,
  initialTitle = '',
): boolean {
  return (
    state.submitting ||
    state.title.trim() !== initialTitle.trim() ||
    Boolean(state.descriptionBlocks?.length) ||
    state.parentId !== null ||
    state.priority !== 'medium' ||
    state.startDate !== '' ||
    state.dueDate !== '' ||
    state.assignToMe ||
    state.appliedTemplate !== null
  );
}
