import type { ReactNode } from 'react';

import { ContentState } from './content-state';

export interface EmptyStateProps {
  title: ReactNode;
  description?: ReactNode;
  action?: {
    label: string;
    onClick?: () => void;
  };
}

export function EmptyState({ title, description, action }: EmptyStateProps) {
  return (
    <ContentState
      action={action}
      description={description}
      kind="empty"
      size="compact"
      title={title}
    />
  );
}
