import { useLayoutEffect, useMemo, useRef, useState } from "react";
import { TranscriptionCostCharts } from "./TranscriptionCostCharts";
import { TranscriptionCostTables } from "./TranscriptionCostTables";
import { ArrowLeftIcon, RefreshCwIcon, ChartColumnIcon } from "lucide-react";
import { PageHeader } from "@/components/shell/PageHeader";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field";
import { Switch } from "@/components/ui/switch";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { EmptyState } from "@/components/ui/empty-state";
import { useTranscriptionCostDetails } from "@/hooks/use-transcription-cost-details";
import {
  aggregateCosts,
  costDateError,
  defaultCostFilters,
  displayedCost,
  formatCost,
  formatCostDuration,
  localDateKey,
  shiftCostDate,
  type CostFilters,
  type CostGranularity,
} from "@/lib/transcription-costs";
import type { TranscriptionCostDetails } from "@/lib/types";

export default function TranscriptionCostsPage({
  onBack,
}: {
  onBack: () => void;
}) {
  const { details, loading, error, refresh } = useTranscriptionCostDetails();
  return (
    <TranscriptionCostsView
      details={details}
      loading={loading}
      error={error}
      onRefresh={() => void refresh()}
      onBack={onBack}
    />
  );
}

export function TranscriptionCostsView({
  details,
  loading,
  error,
  onRefresh,
  onBack,
}: {
  details: TranscriptionCostDetails | null;
  loading: boolean;
  error: string | null;
  onRefresh: () => void;
  onBack: () => void;
}) {
  const pageRef = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    pageRef.current?.scrollIntoView?.({ block: "start" });
  }, []);
  const [filters, setFilters] = useState<CostFilters>(() =>
    defaultCostFilters(),
  );
  const entries = details?.entries;
  const result = useMemo(
    () => aggregateCosts(entries ?? [], filters),
    [entries, filters],
  );
  const providers = useMemo(
    () => [...new Set((entries ?? []).map((entry) => entry.provider))].sort(),
    [entries],
  );
  const models = useMemo(
    () =>
      [
        ...new Set(
          (entries ?? [])
            .filter(
              (entry) =>
                filters.provider === "all" ||
                entry.provider === filters.provider,
            )
            .map((entry) => entry.model_id),
        ),
      ].sort(),
    [entries, filters.provider],
  );
  const rangeError = costDateError(filters.start, filters.end);
  const coverage = result.total.requests
    ? Math.round(
        ((result.total.requests - result.total.unpriced) /
          result.total.requests) *
          100,
      )
    : null;
  const oldest = entries?.reduce<string | null>(
    (value, entry) =>
      value === null || entry.recorded_at < value ? entry.recorded_at : value,
    null,
  );
  const oldestDate =
    oldest && Number.isFinite(new Date(oldest).getTime())
      ? localDateKey(new Date(oldest))
      : null;
  const truncatedRange =
    details &&
    details.entries.length >= details.entry_limit &&
    oldestDate &&
    filters.start <= oldestDate;
  const patchFilters = (patch: Partial<CostFilters>) => {
    setFilters((current) => ({ ...current, ...patch }));
  };
  const chooseRange = (days: number) => {
    const end = localDateKey(new Date());
    patchFilters({ start: shiftCostDate(end, 1 - days), end });
  };

  return (
    <div
      ref={pageRef}
      data-slot="transcription-costs-page"
      className="cost-details-page"
    >
      <PageHeader
        eyebrow="Transcription analytics"
        title="API usage & cost"
        description="Compare providers, understand usage, and find where your transcription budget goes."
        actions={
          <>
            <Button variant="ghost" size="sm" onClick={onBack}>
              <ArrowLeftIcon data-icon="inline-start" />
              Back to Models
            </Button>
            <Button
              variant="outline"
              size="sm"
              disabled={loading}
              onClick={onRefresh}
            >
              <RefreshCwIcon data-icon="inline-start" />
              {loading ? "Refreshing…" : "Refresh"}
            </Button>
          </>
        }
      />
      {error ? (
        <Alert variant="destructive">
          <AlertTitle>Usage could not be updated</AlertTitle>
          <AlertDescription>
            {error}
            {details ? " Showing the last loaded records." : ""}
          </AlertDescription>
        </Alert>
      ) : null}
      <Card>
        <CardHeader>
          <CardTitle>Explore your usage</CardTitle>
          <CardDescription>
            Dates use this Mac’s time zone. Weeks start on Monday; partial weeks
            and months follow your selected range.
          </CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-4">
          <FieldGroup className="cost-details-filters">
            <Field data-invalid={Boolean(rangeError)}>
              <FieldLabel htmlFor="cost-start">Start date</FieldLabel>
              <Input
                id="cost-start"
                type="date"
                value={filters.start}
                aria-invalid={Boolean(rangeError)}
                aria-describedby={rangeError ? "cost-date-error" : undefined}
                onChange={(event) =>
                  patchFilters({ start: event.currentTarget.value })
                }
              />
            </Field>
            <Field data-invalid={Boolean(rangeError)}>
              <FieldLabel htmlFor="cost-end">End date</FieldLabel>
              <Input
                id="cost-end"
                type="date"
                value={filters.end}
                aria-invalid={Boolean(rangeError)}
                aria-describedby={rangeError ? "cost-date-error" : undefined}
                onChange={(event) =>
                  patchFilters({ end: event.currentTarget.value })
                }
              />
            </Field>
            <Field>
              <FieldLabel htmlFor="cost-provider">API provider</FieldLabel>
              <Select
                value={filters.provider}
                onValueChange={(provider) =>
                  patchFilters({ provider, model: "all" })
                }
              >
                <SelectTrigger id="cost-provider" className="w-full">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectGroup>
                    <SelectItem value="all">All providers</SelectItem>
                    {providers.map((provider) => (
                      <SelectItem key={provider} value={provider}>
                        {provider}
                      </SelectItem>
                    ))}
                  </SelectGroup>
                </SelectContent>
              </Select>
            </Field>
            <Field>
              <FieldLabel htmlFor="cost-model">Model</FieldLabel>
              <Select
                value={filters.model}
                onValueChange={(model) => patchFilters({ model })}
              >
                <SelectTrigger id="cost-model" className="w-full">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectGroup>
                    <SelectItem value="all">All models</SelectItem>
                    {models.map((model) => (
                      <SelectItem key={model} value={model}>
                        {model}
                      </SelectItem>
                    ))}
                  </SelectGroup>
                </SelectContent>
              </Select>
            </Field>
          </FieldGroup>
          <div className="cost-details-range-actions">
            <ToggleGroup
              type="single"
              variant="outline"
              value={filters.granularity}
              aria-label="Group usage by"
              onValueChange={(value) => {
                if (value)
                  patchFilters({ granularity: value as CostGranularity });
              }}
            >
              <ToggleGroupItem value="day">Daily</ToggleGroupItem>
              <ToggleGroupItem value="week">Weekly</ToggleGroupItem>
              <ToggleGroupItem value="month">Monthly</ToggleGroupItem>
            </ToggleGroup>
            <div className="flex flex-wrap gap-2">
              <Button variant="ghost" size="sm" onClick={() => chooseRange(7)}>
                Last 7 days
              </Button>
              <Button variant="ghost" size="sm" onClick={() => chooseRange(30)}>
                Last 30 days
              </Button>
              <Button variant="ghost" size="sm" onClick={() => chooseRange(90)}>
                Last 90 days
              </Button>
              <Button
                variant="ghost"
                size="sm"
                onClick={() => {
                  chooseRange(365);
                  patchFilters({ granularity: "month" });
                }}
              >
                Last year
              </Button>
            </div>
          </div>
          {rangeError ? (
            <p
              id="cost-date-error"
              role="alert"
              className="text-sm text-destructive"
            >
              {rangeError}
            </p>
          ) : null}
          <Field orientation="horizontal">
            <FieldContent>
              <FieldLabel htmlFor="cost-reference-rates">
                Include Soniox reference estimates
              </FieldLabel>
              <FieldDescription>
                Estimate missing v5 costs from saved audio: async ≈ $0.10/hour,
                realtime ≈ $0.12/hour. Turn off to see recorded costs only.
              </FieldDescription>
            </FieldContent>
            <Switch
              id="cost-reference-rates"
              checked={Boolean(filters.includeReferenceEstimates)}
              onCheckedChange={(includeReferenceEstimates) =>
                patchFilters({ includeReferenceEstimates })
              }
            />
          </Field>
        </CardContent>
      </Card>
      {!details && loading ? (
        <EmptyState
          title="Loading API usage…"
          description="Reading usage recorded on this Mac."
        />
      ) : !details ? (
        <EmptyState
          title="No usage data loaded"
          description="Refresh to try loading your local records again."
          action={
            <Button variant="outline" onClick={onRefresh}>
              Try again
            </Button>
          }
        />
      ) : rangeError ? null : (
        <>
          <div
            className="cost-details-stats"
            role="group"
            aria-label="Selected period totals"
          >
            <Metric
              title="Estimated cost"
              value={displayedCost(result.total)}
              description={
                result.total.referenceRequests > 0
                  ? `${formatCost(result.total.referenceCost)} from Soniox reference rates.`
                  : result.total.unpriced
                    ? "Known charges only; some requests are not priced."
                    : "USD · local estimate"
              }
            />
            <Metric
              title="Audio processed"
              value={formatCostDuration(result.total.duration)}
              description={`${(result.total.duration / 3_600_000).toFixed(2)} audio hours`}
            />
            <Metric
              title="Transcription requests"
              value={result.total.requests.toLocaleString()}
              description={`${result.groups.length} provider / model combinations`}
            />
            <Metric
              title="Estimated coverage"
              value={coverage === null ? "—" : `${coverage}%`}
              description={`${result.total.unpriced.toLocaleString()} requests without a known cost`}
            />
          </div>
          {truncatedRange ? (
            <Alert>
              <AlertTitle>Earlier usage may no longer be retained</AlertTitle>
              <AlertDescription>
                This Mac keeps up to {details.entry_limit.toLocaleString()}{" "}
                records. Your range includes the oldest retained day,{" "}
                {oldestDate}; earlier records may have been removed, so these
                totals may be incomplete.
              </AlertDescription>
            </Alert>
          ) : null}
          {result.total.unpriced > 0 ? (
            <Alert>
              <AlertTitle>Some costs are not available</AlertTitle>
              <AlertDescription>
                {result.total.unpriced.toLocaleString()} requests have no
                recorded price. Charts show known estimates only; missing prices
                do not mean free usage. Cost per audio hour is shown only when
                all requests in a model group are priced.
              </AlertDescription>
            </Alert>
          ) : null}
          {result.total.referenceRequests > 0 ? (
            <Alert>
              <AlertTitle>
                Includes duration-based reference estimates
              </AlertTitle>
              <AlertDescription>
                {result.total.referenceRequests.toLocaleString()} Soniox
                requests use published hourly equivalents, checked September 11,
                2026. Soniox bills by audio and text tokens; context, session
                duration and historical rates can change actual charges.
                Original records are preserved. Source: soniox.com/pricing.
              </AlertDescription>
            </Alert>
          ) : null}
          {result.entries.length === 0 ? (
            <EmptyState
              icon={ChartColumnIcon}
              title="No API usage in this range"
              description={
                entries?.length
                  ? "Try another date range or clear the provider and model filters."
                  : "Completed cloud transcription usage will appear here. Local models do not generate API charges."
              }
              action={
                <Button
                  variant="outline"
                  onClick={() => {
                    setFilters(defaultCostFilters());
                  }}
                >
                  Reset filters
                </Button>
              }
            />
          ) : (
            <>
              <TranscriptionCostCharts
                result={result}
                providers={providers.filter(
                  (provider) =>
                    filters.provider === "all" || filters.provider === provider,
                )}
                granularity={filters.granularity}
              />
              <TranscriptionCostTables
                key={JSON.stringify(filters)}
                result={result}
                filters={filters}
              />
            </>
          )}
        </>
      )}
      <p className="text-xs text-muted-foreground">
        Local estimates, not a provider invoice. Includes usage recorded by
        WakeNote on this Mac; other apps, unrecorded failures and charges
        without usage records are excluded. Records use their saved date; a
        meeting’s aggregated usage is assigned to its latest saved update.{" "}
        Requests count recorded transcription operations; streaming chunks can
        share a connection.{" "}
        {details
          ? `${details.entries.length.toLocaleString()} saved records · up to ${details.entry_limit.toLocaleString()} retained${oldestDate ? ` · oldest saved record ${oldestDate}` : ""}. `
          : ""}
        Verify final charges in your provider billing dashboard.
      </p>
    </div>
  );
}

function Metric({
  title,
  value,
  description,
}: {
  title: string;
  value: string;
  description: string;
}) {
  return (
    <Card>
      <CardHeader>
        <CardDescription>{title}</CardDescription>
        <CardTitle className="cost-details-metric">{value}</CardTitle>
      </CardHeader>
      <CardContent>
        <p className="text-xs text-muted-foreground">{description}</p>
      </CardContent>
    </Card>
  );
}
