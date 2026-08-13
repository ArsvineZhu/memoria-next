export interface AuthoringDocument {
  title: string;
  body?: string;
  tags?: readonly string[];
}

function escapeAttribute(value: string): string {
  return value.replaceAll("&", "&amp;").replaceAll('"', "&quot;");
}

export function serializeRestrictedMdx(input: AuthoringDocument): string {
  const title = input.title.trim();
  if (!title) {
    throw new Error("authoring title must not be empty");
  }
  const lines = [`# ${title}`];
  for (const tag of input.tags ?? []) {
    const value = tag.trim();
    if (value) {
      lines.push(`<Tag value="${escapeAttribute(value)}"/>`);
    }
  }
  if (input.body) {
    lines.push(input.body.replace(/^\s+|\s+$/g, ""));
  }
  return `${lines.join("\n")}\n`;
}
