import { describe, expect, it } from "vitest";
import { excerpt, shareCard } from "./share";

describe("share cards", () => {
  it("take a line of readable text from the page", () => {
    const html =
      "<html><head><title>T</title><style>p{}</style></head><body><nav>Menu</nav><script>x()</script><h1>Ownership</h1><p>Every value has a single owner, and the value is dropped when its owner goes out of scope.</p></body></html>";
    expect(excerpt(html)).toBe("Ownership Every value has a single owner, and the value is dropped when its owner goes out of scope.");
    expect(excerpt(html, 40)).toBe("Ownership Every value has a single…");
  });

  it("link the title and picture to the published page and escape everything", () => {
    const card = shareCard({
      title: 'Rust <ownership> & "borrowing"',
      url: "https://octo.github.io/herbarium-pages/rust.html?a=1&b=2",
      html: "<p>Moves &amp; borrows&#x21;</p>",
      image: "data:image/png;base64,AAAA",
    });
    expect(card.html).toContain('href="https://octo.github.io/herbarium-pages/rust.html?a=1&amp;b=2"');
    expect(card.html).toContain("Rust &lt;ownership&gt; &amp; &quot;borrowing&quot;");
    expect(card.html).toContain('<img src="data:image/png;base64,AAAA"');
    expect(card.html).toContain("octo.github.io · Herbarium");
    expect(card.text).toBe('Rust <ownership> & "borrowing"\nMoves & borrows!\nhttps://octo.github.io/herbarium-pages/rust.html?a=1&b=2');
  });

  it("work without a link or a picture", () => {
    const card = shareCard({ title: " ", url: null, html: "", image: null });
    expect(card.html).not.toContain("<a ");
    expect(card.html).not.toContain("<img");
    expect(card.text).toBe("Untitled page");
  });
});
