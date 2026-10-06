import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { FormattedText } from "./formatted-text";

describe("FormattedText", () => {
  it("renders the supported inline and block formatting as React elements", () => {
    const { container } = render(
      <FormattedText
        text={[
          "A **bold** and *italic* word with `code`.",
          "",
          "- First item",
          "- Second item",
          "",
          "1. First step",
          "2. Second step",
          "",
          "```ts",
          "const answer = 42;",
          "```",
        ].join("\n")}
      />,
    );

    expect(container.querySelector("strong")?.textContent).toBe("bold");
    expect(container.querySelector("em")?.textContent).toBe("italic");
    expect(container.querySelector("p code")?.textContent).toBe("code");
    expect(container.querySelectorAll("ul li")).toHaveLength(2);
    expect(container.querySelectorAll("ol li")).toHaveLength(2);
    expect(container.querySelector("pre code")?.textContent).toBe("const answer = 42;");
    expect(container.querySelector("pre")?.getAttribute("aria-label")).toBe("ts code");
  });

  it("links only to credential-free http(s) URLs and shows each host", () => {
    const { container } = render(
      <FormattedText
        text="Read [the guide](https://docs.example.test/path?q=1), [unsafe](javascript:alert(1)), and [private](https://user:pass@example.test/)."
      />,
    );

    const links = [...container.querySelectorAll("a")];
    expect(links).toHaveLength(1);
    expect(links[0]?.getAttribute("href")).toBe("https://docs.example.test/path?q=1");
    expect(links[0]?.getAttribute("target")).toBe("_blank");
    expect(links[0]?.getAttribute("rel")).toBe("noopener noreferrer");
    expect(links[0]?.textContent).toBe("the guide · docs.example.test");
    expect(container.textContent).toContain("[unsafe](javascript:alert(1))");
    expect(container.textContent).toContain("[private](https://user:pass@example.test/)");
  });

  it("shows hostile markup, images, and unsupported markdown literally", () => {
    const hostile = [
      '<img src=x onerror="alert(1)">',
      '<script>alert("unsafe")</script>',
      "![remote](https://example.test/tracker.png)",
      "# Unsupported heading",
    ].join("\n\n");
    const { container } = render(<FormattedText text={hostile} />);

    expect(container.querySelector("img, script, h1")).toBeNull();
    expect(container.textContent).toContain('<img src=x onerror="alert(1)">');
    expect(container.textContent).toContain('<script>alert("unsafe")</script>');
    expect(container.textContent).toContain("![remote](https://example.test/tracker.png)");
    expect(container.textContent).toContain("# Unsupported heading");
  });
});
