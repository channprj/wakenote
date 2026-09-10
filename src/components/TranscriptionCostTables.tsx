import { useState } from "react";
import { ChevronLeftIcon, ChevronRightIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  Table,
  TableBody,
  TableCaption,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import {
  displayedCost,
  formatCost,
  formatCostDuration,
  hourlyCost,
  type aggregateCosts,
  type CostFilters,
} from "@/lib/transcription-costs";

const PAGE_SIZE = 14;
export function TranscriptionCostTables({
  result,
  filters,
}: {
  result: ReturnType<typeof aggregateCosts>;
  filters: CostFilters;
}) {
  const [periodPage, setPeriodPage] = useState(1);
  const pages = Math.max(1, Math.ceil(result.buckets.length / PAGE_SIZE));
  const page = Math.min(periodPage, pages);
  const visibleBuckets = [...result.buckets]
    .reverse()
    .slice((page - 1) * PAGE_SIZE, page * PAGE_SIZE);

  return (
    <>
      <Card className="min-w-0">
        <CardHeader>
          <CardTitle>Compare APIs & models</CardTitle>
          <CardDescription>
            Compare your recorded cost per audio hour alongside request volume.
            Different models and workloads can produce different results.
          </CardDescription>
        </CardHeader>
        <CardContent>
          <Table>
            <TableCaption>
              USD estimates for {filters.start} through {filters.end}. “Not
              priced” means the cost is unknown.
            </TableCaption>
            <TableHeader>
              <TableRow>
                <TableHead>API / model</TableHead>
                <TableHead className="text-right">Requests</TableHead>
                <TableHead className="text-right">Audio time</TableHead>
                <TableHead className="text-right">Estimated cost</TableHead>
                <TableHead className="text-right">Cost / audio hour</TableHead>
                <TableHead className="text-right">Unpriced</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {result.groups.map((group) => (
                <TableRow key={JSON.stringify([group.provider, group.model])}>
                  <TableCell>
                    <strong>{group.provider}</strong>
                    <div className="text-xs text-muted-foreground">
                      {group.model}
                    </div>
                    {group.referenceRequests > 0 ? (
                      <div className="text-xs text-muted-foreground">
                        {group.referenceRequests.toLocaleString()} requests use
                        reference estimates
                      </div>
                    ) : null}
                  </TableCell>
                  <TableCell className="text-right tabular-nums">
                    {group.requests.toLocaleString()}
                  </TableCell>
                  <TableCell className="text-right tabular-nums">
                    {formatCostDuration(group.duration)}
                  </TableCell>
                  <TableCell className="text-right tabular-nums">
                    {displayedCost(group)}
                    {group.unpriced > 0 && group.pricedEntries > 0
                      ? " + unknown"
                      : ""}
                  </TableCell>
                  <TableCell className="text-right tabular-nums">
                    {hourlyCost(group) === null
                      ? "—"
                      : formatCost(hourlyCost(group)!)}
                  </TableCell>
                  <TableCell className="text-right tabular-nums">
                    {group.unpriced.toLocaleString()}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </CardContent>
      </Card>
      <Card className="min-w-0">
        <CardHeader>
          <CardTitle>Period breakdown</CardTitle>
          <CardDescription>
            Exact values behind the charts, including periods with no requests.
            Newest first.
          </CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-4">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Period</TableHead>
                <TableHead className="text-right">Estimated cost</TableHead>
                <TableHead className="text-right">Audio time</TableHead>
                <TableHead className="text-right">Requests</TableHead>
                <TableHead className="text-right">Unpriced</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {visibleBuckets.map((bucket) => (
                <TableRow key={bucket.date}>
                  <TableCell>
                    {bucket.date}
                    {bucket.date !== bucket.end ? ` – ${bucket.end}` : ""}
                  </TableCell>
                  <TableCell className="text-right tabular-nums">
                    {displayedCost(bucket)}
                    {bucket.unpriced > 0 && bucket.pricedEntries > 0
                      ? " + unknown"
                      : ""}
                  </TableCell>
                  <TableCell className="text-right tabular-nums">
                    {formatCostDuration(bucket.duration)}
                  </TableCell>
                  <TableCell className="text-right tabular-nums">
                    {bucket.requests.toLocaleString()}
                  </TableCell>
                  <TableCell className="text-right tabular-nums">
                    {bucket.unpriced.toLocaleString()}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
          <div className="cost-details-range-actions">
            <p className="text-sm text-muted-foreground" role="status">
              Page {page} of {pages} · {result.buckets.length} periods
            </p>
            <div className="flex gap-2">
              <Button
                size="sm"
                variant="outline"
                disabled={page === 1}
                onClick={() => setPeriodPage(page - 1)}
              >
                <ChevronLeftIcon data-icon="inline-start" />
                Previous
              </Button>
              <Button
                size="sm"
                variant="outline"
                disabled={page >= pages}
                onClick={() => setPeriodPage(page + 1)}
              >
                Next
                <ChevronRightIcon data-icon="inline-end" />
              </Button>
            </div>
          </div>
        </CardContent>
      </Card>
    </>
  );
}
