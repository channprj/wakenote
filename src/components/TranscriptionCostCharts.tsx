import { useState } from "react";
import {
  Bar,
  BarChart,
  CartesianGrid,
  Line,
  LineChart,
  XAxis,
  YAxis,
} from "recharts";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  ChartContainer,
  ChartLegend,
  ChartLegendContent,
  ChartTooltip,
  ChartTooltipContent,
  type ChartConfig,
} from "@/components/ui/chart";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { EmptyState } from "@/components/ui/empty-state";
import {
  formatCost,
  formatCostTick,
  type aggregateCosts,
  type CostGranularity,
} from "@/lib/transcription-costs";

export function TranscriptionCostCharts({
  result,
  providers,
  granularity,
}: {
  result: ReturnType<typeof aggregateCosts>;
  providers: string[];
  granularity: CostGranularity;
}) {
  const [trend, setTrend] = useState<"cumulative" | "minutes">("cumulative");
  const series = providers.map((provider, index) => ({
    provider,
    key: `provider${index}`,
    color: `var(--chart-${(index % 5) + 1})`,
  }));
  const config: ChartConfig = Object.fromEntries(
    series.map(({ provider, key, color }) => [key, { label: provider, color }]),
  );
  const chartData = result.buckets.map((bucket) => ({
    date: bucket.date,
    label:
      bucket.date === bucket.end
        ? bucket.date
        : `${bucket.date} – ${bucket.end}`,
    cumulative: bucket.cumulative,
    minutes: bucket.duration / 60_000,
    ...Object.fromEntries(
      series.map(({ provider, key }) => [
        key,
        Object.hasOwn(bucket.providers, provider)
          ? bucket.providers[provider]
          : 0,
      ]),
    ),
  }));

  return (
    <div className="cost-details-charts">
      <Card className="min-w-0">
        <CardHeader>
          <CardTitle>Cost by API provider</CardTitle>
          <CardDescription>
            Estimated USD per {granularity}. Hover or use arrow keys to inspect
            values.
          </CardDescription>
        </CardHeader>
        <CardContent>
          {result.total.pricedEntries ? (
            <ChartContainer
              role="group"
              config={config}
              className="cost-details-chart"
              aria-label="Estimated cost by provider and period"
            >
              <BarChart
                accessibilityLayer
                data={chartData}
                margin={{ left: 0, right: 12 }}
              >
                <CartesianGrid vertical={false} />
                <XAxis
                  dataKey="date"
                  tickLine={false}
                  axisLine={false}
                  minTickGap={24}
                  tickFormatter={(value: string) => value.slice(5)}
                />
                <YAxis
                  tickLine={false}
                  axisLine={false}
                  width={56}
                  tickFormatter={formatCostTick}
                />
                <ChartTooltip
                  content={
                    <ChartTooltipContent
                      labelFormatter={(_, payload) =>
                        payload[0]?.payload?.label
                      }
                      formatter={(value, name) => (
                        <span>
                          {config[String(name)]?.label}:{" "}
                          {formatCost(Number(value))}
                        </span>
                      )}
                    />
                  }
                />
                <ChartLegend content={<ChartLegendContent />} />
                {series.map(({ key }) => (
                  <Bar
                    key={key}
                    dataKey={key}
                    fill={`var(--color-${key})`}
                    stackId="cost"
                    maxBarSize={40}
                    isAnimationActive={false}
                  />
                ))}
              </BarChart>
            </ChartContainer>
          ) : (
            <EmptyState
              title="No cost estimates"
              description="Usage is recorded, but no prices are available for these requests."
            />
          )}
        </CardContent>
      </Card>
      <Card className="min-w-0">
        <CardHeader>
          <CardTitle>
            {trend === "cumulative"
              ? "Cumulative estimated cost"
              : "Audio usage over time"}
          </CardTitle>
          <CardDescription>
            {trend === "cumulative"
              ? "Running total within your selected range, in USD."
              : "Audio minutes processed in each period."}
          </CardDescription>
          <ToggleGroup
            type="single"
            variant="outline"
            size="sm"
            value={trend}
            aria-label="Trend metric"
            onValueChange={(value) => {
              if (value === "cumulative" || value === "minutes")
                setTrend(value);
            }}
          >
            <ToggleGroupItem value="cumulative">
              Cumulative cost
            </ToggleGroupItem>
            <ToggleGroupItem value="minutes">Audio time</ToggleGroupItem>
          </ToggleGroup>
        </CardHeader>
        <CardContent>
          {trend === "minutes" || result.total.pricedEntries ? (
            <ChartContainer
              role="group"
              config={{
                [trend]: {
                  label:
                    trend === "cumulative" ? "Estimated cost" : "Audio minutes",
                  color: "var(--primary)",
                },
              }}
              className="cost-details-chart"
              aria-label={
                trend === "cumulative"
                  ? "Cumulative estimated cost trend"
                  : "Audio minutes trend"
              }
            >
              <LineChart
                accessibilityLayer
                data={chartData}
                margin={{ left: 0, right: 12 }}
              >
                <CartesianGrid vertical={false} />
                <XAxis
                  dataKey="date"
                  tickLine={false}
                  axisLine={false}
                  minTickGap={24}
                  tickFormatter={(value: string) => value.slice(5)}
                />
                <YAxis
                  tickLine={false}
                  axisLine={false}
                  width={56}
                  tickFormatter={(value: number) =>
                    trend === "cumulative"
                      ? formatCostTick(value)
                      : value.toFixed(0)
                  }
                />
                <ChartTooltip
                  content={
                    <ChartTooltipContent
                      labelFormatter={(_, payload) =>
                        payload[0]?.payload?.label
                      }
                      formatter={(value) => (
                        <span>
                          {trend === "cumulative"
                            ? formatCost(Number(value))
                            : `${Number(value).toFixed(2)} min`}
                        </span>
                      )}
                    />
                  }
                />
                <Line
                  type="linear"
                  dataKey={trend}
                  stroke={`var(--color-${trend})`}
                  strokeWidth={2}
                  dot={chartData.length < 32}
                  isAnimationActive={false}
                />
              </LineChart>
            </ChartContainer>
          ) : (
            <EmptyState
              title="No cost estimates"
              description="Select Audio time to compare recorded usage."
            />
          )}
        </CardContent>
      </Card>
    </div>
  );
}
