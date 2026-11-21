// A gallery of the AI-DO primitives next to the miy composite components, built
// the way an app builds them: Tailwind entry CSS, platform tokens, common UI.
import {
  StrictMode,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import { createRoot } from "react-dom/client";
import "./app.css";
import "../../tokens/tokens.css";
import {
  AppShell,
  Badge,
  BarChartCard,
  Button,
  ContentState,
  ContextNote,
  DataTable,
  DataTableToolbar,
  DetailDrawer,
  Dialog,
  DirectoryPicker,
  DonutChartCard,
  DropdownMenu,
  EmptyState,
  FeedbackProvider,
  FileTransferPanel,
  FilterBar,
  FormMessage,
  IconButton,
  Input,
  LineChartCard,
  MetricInline,
  Panel,
  RangeSlider,
  RangeSliderRange,
  RangeSliderThumb,
  RangeSliderTrack,
  ScrollArea,
  SearchField,
  Select,
  ShareEditor,
  SidebarNav,
  Skeleton,
  SplitPane,
  StatusBadge,
  StatusSlot,
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
  Toolbar,
  Tooltip,
  Topbar,
  useConfirm,
  useFeedback,
  usePrompt,
  type DataTableColumn,
  type DirectoryOption,
  type DirectoryProvider,
  type FileMetadata,
  type FileService,
  type FileTransferState,
  type ShareProvider,
  type ShareSnapshot,
} from "../src";

declare global {
  interface Window {
    primitives: { results: string[]; unmountOverlays?: () => void };
  }
}
window.primitives = { results: [] };
const record = (value: string) => window.primitives.results.push(value);

const people: DirectoryOption[] = [
  { kind: "user", id: "me", name: "내 이름", description: "기획팀" },
  { kind: "user", id: "design", name: "설계 담당자", description: "설계팀" },
  { kind: "user", id: "review", name: "검토 담당자", description: "품질팀" },
];
const directory: DirectoryProvider = {
  load: async ({ kind, query }) => ({
    items:
      kind === "group"
        ? [{ kind: "group", id: "design-team", name: "설계 그룹" }]
        : people.filter((item) => item.name.includes(query)),
    nextCursor: null,
  }),
};
function shareProvider(): ShareProvider {
  let current: ShareSnapshot = {
    revision: "1",
    entries: [
      { subject: people[0], role: "owner", locked: true },
      { subject: people[1], role: "viewer" },
    ],
    roles: [
      { id: "viewer", label: "보기" },
      { id: "editor", label: "편집" },
      { id: "owner", label: "소유자" },
    ],
    defaultRole: "viewer",
    canManage: true,
  };
  return {
    load: async () => current,
    replace: async ({ entries }) => {
      current = {
        ...current,
        revision: String(Number(current.revision) + 1),
        entries,
      };
      return current;
    },
    lookupResult: async () => null,
  };
}
const attached: FileMetadata = {
  id: "file-1",
  version: 1,
  name: "설계 검토서.pdf",
  mime: "application/pdf",
  size: 482_113,
  sha256: null,
  status: "attached",
};
const transfers: FileTransferState[] = [
  {
    clientId: "client-1",
    requestId: "request-1",
    attachRequestId: "attach-1",
    name: attached.name,
    mime: attached.mime,
    size: attached.size,
    status: "complete",
    progress: 100,
    metadata: attached,
  },
];
const files: FileService = {
  create: async (input) => ({
    ...attached,
    ...input,
    id: input.requestId,
    status: "pending",
  }),
  upload: async (id, file) => ({
    ...attached,
    id,
    name: file.name,
    size: file.size,
    status: "ready",
  }),
  byRequest: async () => null,
  get: async () => attached,
  attach: async (id) => ({ ...attached, id }),
  cancel: async (id) => ({ ...attached, id, status: "cancelled" }),
  download: async () => new Blob([new Uint8Array(attached.size)]),
};

type Row = { id: string; name: string; owner: string; count: number };
const rows: Row[] = [
  { id: "a", name: "설비 점검", owner: "설계 담당자", count: 12 },
  { id: "b", name: "도면 검토", owner: "검토 담당자", count: 3 },
  { id: "c", name: "자재 발주", owner: "내 이름", count: 27 },
];
const columns: DataTableColumn<Row>[] = [
  { accessorKey: "name", header: "업무" },
  { accessorKey: "owner", header: "담당" },
  { accessorKey: "count", header: "건수" },
];
const months = ["7월", "8월", "9월", "10월"];
const series = [
  {
    key: "done",
    label: "완료",
    color: "var(--ui-color-chart-1)",
    data: [12, 18, 15, 24],
  },
  {
    key: "open",
    label: "진행",
    color: "var(--ui-color-chart-2)",
    data: [7, 9, 14, 11],
  },
];

function Overlays() {
  const { confirm, confirmDialog } = useConfirm();
  const { prompt, promptDialog } = usePrompt();
  const feedback = useFeedback();
  const [dialog, setDialog] = useState(false);
  const [drawer, setDrawer] = useState(false);
  const [result, setResult] = useState("");
  const share = useMemo(shareProvider, []);
  const settle = (value: unknown) => {
    record(String(value));
    setResult(String(value));
  };
  return (
    <Panel
      eyebrow="Overlay · Feedback"
      title="대화상자와 알림"
      description="Radix 기반 Dialog·DetailDrawer·DropdownMenu·Tooltip, useConfirm·usePrompt, 토스트"
    >
      <Toolbar
        leading={
          <>
            <Button onClick={() => setDialog(true)}>대화상자 열기</Button>
            <Button onClick={() => setDrawer(true)}>공유 서랍 열기</Button>
            <Button
              onClick={async () =>
                settle(
                  await confirm({
                    title: "항목 삭제",
                    description: "이 항목을 삭제할까요?",
                    confirmLabel: "삭제",
                    cancelLabel: "취소",
                    variant: "danger",
                  }),
                )
              }
            >
              삭제 확인
            </Button>
            <Button
              onClick={async () =>
                settle(
                  await prompt({
                    title: "이름 변경",
                    description: "새 이름을 입력해 주세요.",
                    placeholder: "이름",
                    defaultValue: "기존 이름",
                    submitLabel: "변경",
                    cancelLabel: "취소",
                  }),
                )
              }
            >
              이름 입력
            </Button>
            <Button
              variant="primary"
              onClick={() =>
                feedback.success("저장했습니다", "변경 내용을 반영했습니다.")
              }
            >
              저장 알림
            </Button>
            <Button
              variant="subtle"
              onClick={() =>
                feedback.error(
                  "저장하지 못했습니다",
                  "잠시 후 다시 시도해 주세요.",
                )
              }
            >
              오류 알림
            </Button>
          </>
        }
        trailing={
          <>
            <Tooltip content="마지막 동기화 10:42">
              <Button variant="ghost">동기화 정보</Button>
            </Tooltip>
            <DropdownMenu
              trigger={<Button>더 보기 메뉴</Button>}
              side="bottom"
              items={[
                { id: "copy", label: "복제", onSelect: () => settle("copy") },
                { id: "move", label: "이동", disabled: true },
                {
                  id: "remove",
                  label: "삭제",
                  tone: "danger",
                  separatorBefore: true,
                  onSelect: () => settle("remove"),
                },
              ]}
            />
          </>
        }
      />
      <output data-testid="overlay-result" className="mt-3 block">
        {result}
      </output>
      <Dialog
        open={dialog}
        onOpenChange={setDialog}
        title="설정 변경"
        description="변경한 내용은 저장해야 적용됩니다."
        closeLabel="닫기"
        actions={
          <>
            <Button variant="ghost" onClick={() => setDialog(false)}>
              취소
            </Button>
            <Button variant="primary" onClick={() => setDialog(false)}>
              저장
            </Button>
          </>
        }
      >
        <label className="grid gap-1.5 text-[length:var(--ui-text-caption)] font-semibold">
          표시 이름
          <Input defaultValue="품질 개선 프로젝트" />
        </label>
      </Dialog>
      <DetailDrawer
        open={drawer}
        onOpenChange={setDrawer}
        title="공유 설정"
        description="miy 조합 컴포넌트를 AI-DO 서랍 안에 둔 예"
        closeLabel="서랍 닫기"
      >
        <div className="pt-3">
          <ShareEditor
            provider={share}
            directoryProvider={directory}
            currentUserId="me"
          />
        </div>
      </DetailDrawer>
      {confirmDialog}
      {promptDialog}
    </Panel>
  );
}

function Gallery() {
  const [filter, setFilter] = useState("all");
  const [role, setRole] = useState("viewer");
  const [progress, setProgress] = useState(40);
  const [selected, setSelected] = useState<string>();
  const [subjects, setSubjects] = useState<DirectoryOption[]>([people[1]]);
  const [overlays, setOverlays] = useState(true);
  window.primitives.unmountOverlays = () => setOverlays(false);
  return (
    <main className="mx-auto grid max-w-[1100px] gap-4 p-4">
      <Topbar
        breadcrumb="공용 UI 2.0.0"
        title="AI-DO 기본 요소와 miy 조합 컴포넌트"
        description="Tailwind CSS 4 · Radix UI · 플랫폼 토큰"
        actions={
          <>
            <Badge
              data-testid="dark-variant"
              tone="accent"
              className="hidden dark:inline-flex"
            >
              다크
            </Badge>
            <ThemeToggle />
          </>
        }
      />
      <Panel
        eyebrow="Primitives"
        title="버튼과 배지"
        description="Button 변형·크기, IconButton, Badge 색조"
        actions={<Badge tone="accent">2.0.0</Badge>}
        status={<StatusBadge>동기화됨</StatusBadge>}
      >
        <div className="grid gap-3">
          <div className="flex flex-wrap items-center gap-2">
            <Button variant="primary">기본 동작</Button>
            <Button>보조 동작</Button>
            <Button variant="ghost">고스트</Button>
            <Button variant="subtle">서브틀</Button>
            <Button size="comfortable">넉넉한 크기</Button>
            <Button disabled>비활성</Button>
            <IconButton aria-label="항목 추가">+</IconButton>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <Badge>중립</Badge>
            <Badge tone="accent">강조</Badge>
            <Badge tone="success">완료</Badge>
            <Badge tone="warning">주의</Badge>
            <Badge tone="danger">위험</Badge>
            <span className="rounded-[var(--ui-radius-sm)] bg-[var(--ui-color-bg-strong)] p-1">
              <Badge tone="inverse">반전</Badge>
            </span>
          </div>
        </div>
      </Panel>
      <Panel eyebrow="Primitives" title="입력과 선택">
        <div className="grid gap-3 min-[721px]:grid-cols-2">
          <label className="grid gap-1.5 text-[length:var(--ui-text-caption)] font-semibold">
            프로젝트 이름
            <Input
              placeholder="이름을 입력해 주세요"
              aria-describedby="name-message"
            />
            <FormMessage
              id="name-message"
              message="두 글자 이상 입력해 주세요."
            />
          </label>
          <div className="grid content-start gap-1.5">
            <SearchField
              aria-label="업무 검색"
              placeholder="업무 검색"
              shortcut="Ctrl K"
            />
            <FilterBar
              options={["all", "mine", "done"].map((id) => ({
                id,
                label: { all: "전체", mine: "내 업무", done: "완료" }[id]!,
                active: filter === id,
                onSelect: () => setFilter(id),
              }))}
            />
          </div>
          <div className="flex items-center gap-3">
            <Select
              value={role}
              onValueChange={setRole}
              options={[
                { value: "viewer", label: "보기" },
                { value: "editor", label: "편집" },
                { value: "owner", label: "소유자" },
              ]}
            />
            <output data-testid="role">{role}</output>
          </div>
          <div className="flex items-center gap-3">
            <RangeSlider
              className="relative flex h-5 w-full touch-none select-none items-center"
              value={[progress]}
              onValueChange={([value]) => setProgress(value)}
              max={100}
              step={5}
            >
              <RangeSliderTrack className="relative h-1 grow rounded-full bg-ui-surface-subtle">
                <RangeSliderRange className="absolute h-full rounded-full bg-ui-accent" />
              </RangeSliderTrack>
              <RangeSliderThumb
                aria-label="진행률"
                className="block size-4 rounded-full border border-[var(--ui-color-border-strong)] bg-ui-surface-raised"
              />
            </RangeSlider>
            <output data-testid="progress">{progress}</output>
          </div>
        </div>
      </Panel>
      {overlays ? <Overlays /> : null}
      <Panel eyebrow="Data display" title="표와 상태">
        <Tabs defaultValue="table">
          <TabsList aria-label="표시 방식">
            <TabsTrigger value="table">표</TabsTrigger>
            <TabsTrigger value="states">상태</TabsTrigger>
            <TabsTrigger value="charts">차트</TabsTrigger>
          </TabsList>
          <TabsContent value="table">
            <SplitPane
              mobileAsideLabel="요약"
              main={
                <>
                  <DataTableToolbar
                    title="업무 목록"
                    meta="3건"
                    actions={<Button>내보내기</Button>}
                  />
                  <DataTable
                    columns={columns}
                    rows={rows}
                    selection={{
                      selectedRowId: selected,
                      getRowId: (row) => row.id,
                      onRowClick: (row) => setSelected(row.id),
                    }}
                  />
                </>
              }
              aside={
                <>
                  <div className="grid grid-cols-2 gap-3 rounded-[var(--ui-radius-md)] border border-[var(--ui-color-border)] bg-ui-surface p-3">
                    <MetricInline label="전체" value="42" />
                    <MetricInline label="지연" value="3" />
                  </div>
                  <ScrollArea className="h-24 rounded-[var(--ui-radius-md)] border border-[var(--ui-color-border)] bg-ui-surface">
                    <ul className="m-0 grid list-none gap-1 p-3 text-[length:var(--ui-text-caption)]">
                      {Array.from({ length: 8 }, (_, index) => (
                        <li key={index}>변경 기록 {index + 1}</li>
                      ))}
                    </ul>
                  </ScrollArea>
                </>
              }
            />
          </TabsContent>
          <TabsContent value="states">
            <div className="grid gap-3 min-[721px]:grid-cols-2">
              <ContentState kind="loading" title="불러오는 중" size="compact" />
              <ContentState
                kind="error"
                title="목록을 불러올 수 없습니다"
                description="잠시 후 다시 시도해 주세요."
                action={{ label: "다시 시도", onClick: () => record("retry") }}
                size="compact"
              />
              <ContentState
                kind="unavailable"
                title="접근 권한이 없습니다"
                size="compact"
              />
              <EmptyState
                title="등록된 업무가 없습니다"
                description="업무를 추가하면 여기에 표시됩니다."
              />
              <div className="grid gap-2">
                <Skeleton className="h-8 w-full" />
                <Skeleton className="h-8 w-2/3" />
              </div>
              <div className="grid content-start gap-2">
                <StatusSlot tone="success" message="저장했습니다." />
                <StatusSlot
                  tone="error"
                  announce="assertive"
                  message="저장하지 못했습니다."
                  action={{
                    label: "다시 시도",
                    onClick: () => record("again"),
                  }}
                />
                <ContextNote tone="warning" title="확인 필요">
                  공유 대상이 바뀌면 기존 링크가 만료됩니다.
                </ContextNote>
              </div>
            </div>
          </TabsContent>
          <TabsContent value="charts">
            <div className="grid gap-3 min-[721px]:grid-cols-3">
              <LineChartCard
                title="월별 추이"
                categories={months}
                series={series}
              />
              <BarChartCard
                title="월별 건수"
                categories={months}
                series={series}
              />
              <DonutChartCard
                title="완료 비율"
                categories={months}
                series={series}
              />
            </div>
          </TabsContent>
        </Tabs>
      </Panel>
      <Panel
        eyebrow="miy"
        title="조합 컴포넌트"
        description="DirectoryPicker · FileTransferPanel"
      >
        <div className="grid gap-4 min-[721px]:grid-cols-2">
          <DirectoryPicker
            label="참여자"
            provider={directory}
            value={subjects}
            onChange={setSubjects}
            currentUserId="me"
          />
          <FileTransferPanel
            service={files}
            reference="project:gallery"
            initialTransfers={transfers}
          />
        </div>
      </Panel>
    </main>
  );
}

function ThemeToggle() {
  const [dark, setDark] = useState(
    new URL(location.href).searchParams.get("theme") === "dark",
  );
  // The frame runtime sets both on the document root; so does this fixture.
  useEffect(() => {
    const root = document.documentElement;
    root.classList.toggle("dark", dark);
    root.dataset.miyTheme = dark ? "dark" : "light";
  }, [dark]);
  return <Button onClick={() => setDark((value) => !value)}>테마 전환</Button>;
}

function Shell({ children }: { children: ReactNode }) {
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState("tasks");
  const item = (id: string, label: string, hint?: string) => ({
    id,
    label,
    hint,
    active: active === id,
    onSelect: () => setActive(id),
  });
  return (
    <AppShell
      mobileSidebar={{
        open,
        onOpenChange: setOpen,
        label: "메뉴",
        closeLabel: "메뉴 닫기",
        brandEyebrow: "miy",
        brandTitle: "업무 앱",
      }}
      sidebar={
        <SidebarNav
          ariaLabel="앱 메뉴"
          brand={{ eyebrow: "miy", title: "업무 앱" }}
          launcher={{ label: "빠른 이동", hint: "Ctrl K" }}
          sections={[
            {
              id: "work",
              label: "업무",
              items: [item("tasks", "업무 목록", "12"), item("files", "파일")],
            },
            { id: "admin", label: "관리", items: [item("share", "공유")] },
          ]}
          footerBadges={["2.0.0"]}
        />
      }
    >
      {children}
    </AppShell>
  );
}

const view = new URL(location.href).searchParams.get("view");
createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <FeedbackProvider
      labels={{ close: "알림 닫기", item: "알림", region: "알림 영역" }}
    >
      {view === "shell" ? (
        <Shell>
          <Gallery />
        </Shell>
      ) : (
        <Gallery />
      )}
    </FeedbackProvider>
  </StrictMode>,
);
