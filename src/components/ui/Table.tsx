import { useMemo, useState, type ReactNode } from 'react';

import { Icon } from './Icon';
import { Spinner } from './primitives';
import { cn } from '@/lib/cn';

export interface Column<T> {
  key: string;
  header: ReactNode;
  width?: number | string;
  align?: 'left' | 'right';
  render: (row: T) => ReactNode;
  /** Provide to make the column sortable. */
  sortValue?: (row: T) => string | number;
  className?: string;
}

export interface DataTableProps<T> {
  columns: Array<Column<T>>;
  rows: T[];
  rowKey: (row: T, index: number) => string;
  loading?: boolean;
  emptyTitle?: string;
  emptyText?: ReactNode;
  emptyIcon?: ReactNode;
  onRowClick?: (row: T) => void;
  isSelected?: (row: T) => boolean;
  /** Columns stay put while the body scrolls; requires `.table-wrap`. */
  sticky?: boolean;
  initialSort?: { key: string; direction: 'asc' | 'desc' };
  className?: string;
}

export function DataTable<T>({
  columns,
  rows,
  rowKey,
  loading = false,
  emptyTitle = 'Nothing to show',
  emptyText,
  emptyIcon,
  onRowClick,
  isSelected,
  sticky = true,
  initialSort,
  className,
}: DataTableProps<T>) {
  const [sort, setSort] = useState<{ key: string; direction: 'asc' | 'desc' } | null>(
    initialSort ?? null,
  );

  const sorted = useMemo(() => {
    if (!sort) return rows;
    const column = columns.find((entry) => entry.key === sort.key);
    if (!column?.sortValue) return rows;
    const factor = sort.direction === 'asc' ? 1 : -1;
    return [...rows].sort((left, right) => {
      const a = column.sortValue!(left);
      const b = column.sortValue!(right);
      if (typeof a === 'number' && typeof b === 'number') return (a - b) * factor;
      return String(a).localeCompare(String(b)) * factor;
    });
  }, [rows, sort, columns]);

  const toggleSort = (key: string) => {
    setSort((current) => {
      if (current?.key !== key) return { key, direction: 'asc' };
      if (current.direction === 'asc') return { key, direction: 'desc' };
      return null;
    });
  };

  return (
    <div className="table-wrap">
      <table className={cn('table', sticky && 'table--fixed', className)}>
        <thead>
          <tr>
            {columns.map((column) => (
              <th
                key={column.key}
                style={{ width: column.width }}
                className={cn(
                  column.align === 'right' && 'table__num',
                  column.sortValue && 'sortable',
                )}
                onClick={column.sortValue ? () => toggleSort(column.key) : undefined}
              >
                <span className="row gap-4" style={{ justifyContent: column.align === 'right' ? 'flex-end' : undefined }}>
                  {column.header}
                  {column.sortValue && sort?.key === column.key ? (
                    <Icon name={sort.direction === 'asc' ? 'chevron-down' : 'chevron-right'} size={11} />
                  ) : null}
                </span>
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {sorted.map((row, index) => (
            <tr
              key={rowKey(row, index)}
              className={cn(isSelected?.(row) && 'is-selected', onRowClick && 'is-clickable')}
              onClick={onRowClick ? () => onRowClick(row) : undefined}
              style={onRowClick ? { cursor: 'pointer' } : undefined}
            >
              {columns.map((column) => (
                <td
                  key={column.key}
                  className={cn(column.align === 'right' && 'table__num', column.className)}
                >
                  {column.render(row)}
                </td>
              ))}
            </tr>
          ))}
          {loading ? (
            <tr>
              <td colSpan={columns.length}>
                <div className="table-empty row gap-8 center">
                  <Spinner />
                  Loading…
                </div>
              </td>
            </tr>
          ) : null}
          {!loading && sorted.length === 0 ? (
            <tr>
              <td colSpan={columns.length}>
                <div className="table-empty">
                  {emptyIcon ? <div className="center">{emptyIcon}</div> : null}
                  <div className="bold" style={{ color: 'var(--text-muted)', marginTop: 6 }}>
                    {emptyTitle}
                  </div>
                  {emptyText ? <div className="mt-8">{emptyText}</div> : null}
                </div>
              </td>
            </tr>
          ) : null}
        </tbody>
      </table>
    </div>
  );
}
