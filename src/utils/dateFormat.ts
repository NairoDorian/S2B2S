const parseTimestamp = (timestamp: string): Date | null => {
  const timestampMs = parseInt(timestamp, 10) * 1000;
  const date = new Date(timestampMs);
  return isNaN(date.getTime()) ? null : date;
};

const safeFormat = (
  timestamp: string,
  locale: string,
  format: Intl.DateTimeFormatOptions,
): string => {
  try {
    const date = parseTimestamp(timestamp);
    if (!date) return timestamp;
    return new Intl.DateTimeFormat(locale, format).format(date);
  } catch (error) {
    console.error("Failed to format date:", error);
    return timestamp;
  }
};

export const formatDateTime = (timestamp: string, locale: string): string =>
  safeFormat(timestamp, locale, {
    year: "numeric",
    month: "long",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });

export const formatDate = (timestamp: string, locale: string): string =>
  safeFormat(timestamp, locale, {
    year: "numeric",
    month: "long",
    day: "numeric",
  });

/**
 * Format a date string or timestamp to a relative time string (e.g., "2 hours ago")
 * @param timestamp - Unix timestamp in seconds (as string)
 * @param locale - BCP 47 language tag (e.g., 'en', 'es', 'fr')
 * @returns Relative time string
 */
export const formatRelativeTime = (
  timestamp: string,
  locale: string,
): string => {
  try {
    // Convert Unix timestamp (seconds) to milliseconds
    const timestampMs = parseInt(timestamp, 10) * 1000;
    const date = new Date(timestampMs);
    const now = new Date();

    // Check if date is valid
    if (isNaN(date.getTime())) {
      return timestamp; // Return original if invalid
    }

    const diffInSeconds = Math.floor((now.getTime() - date.getTime()) / 1000);

    // Use Intl.RelativeTimeFormat for proper localization
    const rtf = new Intl.RelativeTimeFormat(locale, { numeric: "auto" });

    // Less than a minute
    if (diffInSeconds < 60) {
      return rtf.format(-diffInSeconds, "second");
    }

    // Less than an hour
    const diffInMinutes = Math.floor(diffInSeconds / 60);
    if (diffInMinutes < 60) {
      return rtf.format(-diffInMinutes, "minute");
    }

    // Less than a day
    const diffInHours = Math.floor(diffInMinutes / 60);
    if (diffInHours < 24) {
      return rtf.format(-diffInHours, "hour");
    }

    // Less than a week
    const diffInDays = Math.floor(diffInHours / 24);
    if (diffInDays < 7) {
      return rtf.format(-diffInDays, "day");
    }

    // Less than a month (30 days)
    if (diffInDays < 30) {
      const diffInWeeks = Math.floor(diffInDays / 7);
      return rtf.format(-diffInWeeks, "week");
    }

    // Less than a year
    if (diffInDays < 365) {
      const diffInMonths = Math.floor(diffInDays / 30);
      return rtf.format(-diffInMonths, "month");
    }

    // More than a year
    const diffInYears = Math.floor(diffInDays / 365);
    return rtf.format(-diffInYears, "year");
  } catch (error) {
    console.error("Failed to format relative time:", error);
    return formatDateTime(timestamp, locale); // Fallback to absolute time
  }
};
