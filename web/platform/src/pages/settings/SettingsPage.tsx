// Ported from AI-DO apps/web/src/platform/auth/ProfilePage.tsx and
// ProfilePageContent.tsx (0002af062): the same layout, navigation and sections
// that miy has (profile, appearance, security).
import { LogOut } from 'lucide-react';
import { useNavigate, useParams } from '@tanstack/react-router';

import { useSurface, useUser, type User } from '../../data/session';
import { leaveForIdentityProvider, signOut } from '../../lib/sign-out';
import { surfaceUrl } from '../../lib/paths';
import { AppearanceSettingsSection } from './AppearanceSettingsSection';
import { ProfileSettingsSection } from './ProfileSettingsSection';
import { SecuritySettingsSection } from './SecuritySettingsSection';
import { NotificationsSettingsSection } from './NotificationsSettingsSection';
import { installationsIn, useInstallations } from '../../data/installations';
import { portalEnvironment } from '../../lib/paths';
import { SettingsNavigation } from './SettingsNavigation';
import {
  SETTINGS_SECTIONS,
  type SettingsSection,
} from './settings-page-model';
import { useSettingsPageController } from './useSettingsPageController';

export function SettingsPage() {
  const user = useUser();
  const params = useParams({ strict: false }) as { section?: string };
  const initialTab = SETTINGS_SECTIONS.includes(params.section as SettingsSection)
    ? (params.section as SettingsSection)
    : 'profile';
  return <SettingsPageContent key={user.id} initialTab={initialTab} user={user} />;
}

function SettingsPageContent({
  initialTab,
  user,
}: {
  initialTab: SettingsSection;
  user: User;
}) {
  const options = useSurface();
  const navigate = useNavigate();
  const installations = useInstallations();
  const ownEnvironment = portalEnvironment(options.surface, options.legacyPortal);
  const apps = ownEnvironment
    ? installationsIn(installations.items, ownEnvironment).map((app) => ({
        appId: app.appId,
        name: app.name,
      }))
    : [];
  const {
    dispatch,
    handleProfileSubmit,
    handleRevoke,
    handleSectionChange,
    i18nLanguage,
    saveDateFormatPreference,
    saveLocalePreference,
    saveThemePreference,
    saveTimeZonePreference,
    security,
    state,
    t,
  } = useSettingsPageController({ initialTab, user });
  const roles = [
    user.admin ? t('portal:roles.admin') : null,
    user.canManageConsole ? t('portal:roles.appManager') : null,
  ].filter((role): role is string => role !== null);

  return (
    <div className="h-full overflow-y-auto custom-scrollbar">
      <div className="mx-auto max-w-5xl p-8 max-[720px]:p-4">
        <div className="mb-8 flex items-center justify-between">
          <h1 className="app-text-title-lg text-app-ink">
            {t('auth:settings.mySettings')}
          </h1>
          <button
            className="app-text-control flex items-center gap-2 rounded-md px-3 py-1.5 text-app-danger transition-colors hover:bg-app-danger/10"
            onClick={() => {
              void signOut(user.csrf);
            }}
            type="button"
          >
            <LogOut size={15} />
            {t('common:actions.signOut')}
          </button>
        </div>

        <div className="grid grid-cols-[200px_1fr] gap-10 max-[820px]:grid-cols-1 max-[820px]:gap-6">
          <SettingsNavigation
            activeSection={state.activeSection}
            onSectionChange={(section) => {
              handleSectionChange(section);
              void navigate({ to: `/settings/${section}` as '/', replace: true });
            }}
            t={t}
          />
          <div className="min-w-0">
            {state.activeSection === 'profile' ? (
              <ProfileSettingsSection
                dispatch={dispatch}
                onSubmit={(event) => {
                  void handleProfileSubmit(event);
                }}
                roles={roles.length > 0 ? roles : [t('portal:roles.member')]}
                state={state}
                t={t}
                user={user}
              />
            ) : null}
            {state.activeSection === 'appearance' ? (
              <AppearanceSettingsSection
                onDateFormatChange={(value) => {
                  void saveDateFormatPreference(value);
                }}
                onLocaleChange={(value) => {
                  void saveLocalePreference(value);
                }}
                onThemeChange={(value) => {
                  void saveThemePreference(value);
                }}
                onTimeZoneChange={(value) => {
                  void saveTimeZonePreference(value);
                }}
                state={state}
                t={t}
              />
            ) : null}
            {state.activeSection === 'notifications' ? (
              <NotificationsSettingsSection
                apps={apps}
                csrf={user.csrf}
                hasInbox={ownEnvironment !== null}
                t={t}
              />
            ) : null}
            {state.activeSection === 'security' ? (
              <SecuritySettingsSection
                cliSessionsHref={surfaceUrl(options.consoleOrigin, '/cli/sessions')}
                i18nLanguage={i18nLanguage}
                onChangePassword={() => {
                  if (options.passwordUrl)
                    leaveForIdentityProvider(options.passwordUrl);
                }}
                onRevoke={handleRevoke}
                passwordUrl={options.passwordUrl}
                state={security}
                t={t}
                timeZone={state.timeZone}
              />
            ) : null}
          </div>
        </div>
      </div>
    </div>
  );
}
