export type DiffSegment = {
  text: string;
  kind: "unchanged" | "removed" | "added";
};

type Token = {
  value: string;
  normalized: string;
};

const TOKEN_SPLIT = /(\s+|[^\p{L}\p{N}_\s]+)/u;

function tokenize(text: string): Token[] {
  return text
    .split(TOKEN_SPLIT)
    .filter((part) => part.length > 0)
    .map((part) => ({
      value: part,
      normalized: /\s+/u.test(part) ? part : part.toLocaleLowerCase(),
    }));
}

function mergeSegments(segments: DiffSegment[]) {
  const merged: DiffSegment[] = [];
  for (const segment of segments) {
    if (!segment.text) {
      continue;
    }
    const previous = merged.at(-1);
    if (previous && previous.kind === segment.kind) {
      previous.text += segment.text;
    } else {
      merged.push({ ...segment });
    }
  }
  return merged;
}

export function buildInlineDiff(
  referenceText: string,
  alternativeText: string,
): { reference: DiffSegment[]; alternative: DiffSegment[] } {
  const referenceTokens = tokenize(referenceText);
  const alternativeTokens = tokenize(alternativeText);
  const rows = referenceTokens.length + 1;
  const cols = alternativeTokens.length + 1;
  const lcs = Array.from({ length: rows }, () => Array<number>(cols).fill(0));

  for (let refIndex = referenceTokens.length - 1; refIndex >= 0; refIndex -= 1) {
    for (let altIndex = alternativeTokens.length - 1; altIndex >= 0; altIndex -= 1) {
      if (referenceTokens[refIndex].normalized === alternativeTokens[altIndex].normalized) {
        lcs[refIndex][altIndex] = lcs[refIndex + 1][altIndex + 1] + 1;
      } else {
        lcs[refIndex][altIndex] = Math.max(
          lcs[refIndex + 1][altIndex],
          lcs[refIndex][altIndex + 1],
        );
      }
    }
  }

  const referenceSegments: DiffSegment[] = [];
  const alternativeSegments: DiffSegment[] = [];
  let refIndex = 0;
  let altIndex = 0;

  while (refIndex < referenceTokens.length && altIndex < alternativeTokens.length) {
    if (referenceTokens[refIndex].normalized === alternativeTokens[altIndex].normalized) {
      referenceSegments.push({ text: referenceTokens[refIndex].value, kind: "unchanged" });
      alternativeSegments.push({ text: alternativeTokens[altIndex].value, kind: "unchanged" });
      refIndex += 1;
      altIndex += 1;
      continue;
    }

    if (lcs[refIndex + 1][altIndex] >= lcs[refIndex][altIndex + 1]) {
      referenceSegments.push({ text: referenceTokens[refIndex].value, kind: "removed" });
      refIndex += 1;
    } else {
      alternativeSegments.push({ text: alternativeTokens[altIndex].value, kind: "added" });
      altIndex += 1;
    }
  }

  while (refIndex < referenceTokens.length) {
    referenceSegments.push({ text: referenceTokens[refIndex].value, kind: "removed" });
    refIndex += 1;
  }

  while (altIndex < alternativeTokens.length) {
    alternativeSegments.push({ text: alternativeTokens[altIndex].value, kind: "added" });
    altIndex += 1;
  }

  return {
    reference: mergeSegments(referenceSegments),
    alternative: mergeSegments(alternativeSegments),
  };
}
