import { describe, expect, it } from "vitest";
import { fileName, folderOf, formatBytes, formatDuration, formatLength, formatSize, formatWhen, tildify } from "./format";

describe("formatDuration", () => {
  it("formats like a stopwatch", () => {
    expect(formatDuration(0)).toBe("0:00");
    expect(formatDuration(7.9)).toBe("0:07");
    expect(formatDuration(65)).toBe("1:05");
    expect(formatDuration(3723)).toBe("1:02:03");
    expect(formatDuration(-3)).toBe("0:00");
  });
});

describe("formatBytes", () => {
  it("picks a sensible unit", () => {
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(812 * 1024)).toBe("812 KB");
    expect(formatBytes(4.2 * 1024 * 1024)).toBe("4.2 MB");
    expect(formatBytes(42 * 1024 * 1024)).toBe("42 MB");
    expect(formatBytes(null)).toBe("");
  });
});

describe("formatWhen", () => {
  const now = new Date(2026, 10, 25, 15, 0, 0);
  it("says Today and Yesterday", () => {
    expect(formatWhen(new Date(2026, 10, 25, 14, 3).toISOString(), now)).toMatch(/^Today /);
    expect(formatWhen(new Date(2026, 10, 24, 9, 12).toISOString(), now)).toMatch(/^Yesterday /);
    expect(formatWhen(new Date(2026, 10, 20, 9, 12).toISOString(), now)).toMatch(/2026/);
    expect(formatWhen("nope", now)).toBe("");
  });
});

describe("tildify", () => {
  it("shortens home paths", () => {
    expect(tildify("/Users/me/Downloads")).toBe("~/Downloads");
    expect(tildify("/home/me")).toBe("~");
    expect(tildify("/Volumes/Data/rec")).toBe("/Volumes/Data/rec");
  });
});

describe("History formatting", () => {
  it("shows lengths in seconds or minutes", () => {
    expect(formatLength(12.4)).toBe("12 sec");
    expect(formatLength(63)).toBe("1:03 min");
    expect(formatLength(3723)).toBe("1:02:03");
  });

  it("shows compact sizes", () => {
    expect(formatSize(6 * 1024)).toBe("6KB");
    expect(formatSize(4.2 * 1024 * 1024)).toBe("4.2MB");
    expect(formatSize(null)).toBe("");
  });

  it("splits paths on either separator", () => {
    expect(folderOf("/Users/me/Downloads/rec-1.mp4")).toBe("/Users/me/Downloads");
    expect(fileName("/Users/me/Downloads/rec-1.mp4")).toBe("rec-1.mp4");
    expect(folderOf("C:\\Users\\me\\Downloads\\rec-1.mp4")).toBe("C:\\Users\\me\\Downloads");
    expect(fileName("C:\\Users\\me\\Downloads\\rec-1.mp4")).toBe("rec-1.mp4");
  });
});
