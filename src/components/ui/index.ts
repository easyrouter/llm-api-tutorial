/**
 * Shared UI kit. Dumb, presentational components. The exceptions are `FixActionButtons` and
 * the one-click dialogs it owns (`PathRepairDialog`, `EnvCleanupDialog`), which talk to the
 * wizard store and the IPC layer because every screen needs exactly that behaviour.
 * Import from `@/components/ui`.
 */
export { Alert } from "./Alert";
export type { AlertProps, AlertVariant } from "./Alert";
export { Button } from "./Button";
export type { ButtonProps } from "./Button";
export { Card } from "./Card";
export type { CardProps } from "./Card";
export { ConfirmDialog } from "./ConfirmDialog";
export type { ConfirmDialogProps } from "./ConfirmDialog";
export { CopyField } from "./CopyField";
export type { CopyFieldProps } from "./CopyField";
export { EnvCleanupDialog } from "./EnvCleanupDialog";
export type { EnvCleanupDialogProps } from "./EnvCleanupDialog";
export { ErrorBanner } from "./ErrorBanner";
export type { ErrorBannerProps } from "./ErrorBanner";
export { ExternalLink } from "./ExternalLink";
export type { ExternalLinkProps } from "./ExternalLink";
export { FixActionButtons } from "./FixActionButtons";
export type { FixActionButtonsProps } from "./FixActionButtons";
export { KeyValueList } from "./KeyValueList";
export type { KeyValueItem, KeyValueListProps } from "./KeyValueList";
export { LogView, MAX_RENDERED_LOG_LINES } from "./LogView";
export type { LogLine, LogViewProps } from "./LogView";
export { PathRepairDialog } from "./PathRepairDialog";
export type { PathRepairDialogProps } from "./PathRepairDialog";
export { ProgressBar } from "./ProgressBar";
export type { ProgressBarProps } from "./ProgressBar";
export { Spinner } from "./Spinner";
export type { SpinnerProps, SpinnerSize } from "./Spinner";
export { StatusBadge } from "./StatusBadge";
export type { BadgeStatus, StatusBadgeProps } from "./StatusBadge";
export { StepFooter } from "./StepFooter";
export type { StepFooterProps } from "./StepFooter";
