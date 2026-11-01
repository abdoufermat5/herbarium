<script lang="ts" module>
  export type IllustrationName =
    | "seedling"
    | "pressed-flower"
    | "magnifier-leaf"
    | "watering-can"
    | "empty-pot"
    | "sprouting-book";

  type Stroke = string | { d: string; transform: string };
  interface Drawing {
    /** The one pastel shape behind the sketch, offset from the drawing's centre. */
    wash: string;
    /** `-soft` colour token behind the sketch. */
    tint: "sage" | "rose" | "amber" | "sky" | "clay" | "teal";
    strokes: Stroke[];
  }

  const circle = (cx: number, cy: number, r: number) =>
    `M${cx - r} ${cy}a${r} ${r} 0 1 0 ${2 * r} 0a${r} ${r} 0 1 0 ${-2 * r} 0Z`;
  const roundRect = (x: number, y: number, w: number, h: number, r: number) =>
    `M${x + r} ${y}H${x + w - r}a${r} ${r} 0 0 1 ${r} ${r}V${y + h - r}a${r} ${r} 0 0 1 ${-r} ${r}H${x + r}` +
    `a${r} ${r} 0 0 1 ${-r} ${-r}V${y + r}a${r} ${r} 0 0 1 ${r} ${-r}Z`;
  const BLOB =
    "M24 58C20 36 46 20 72 24C98 28 108 56 96 78C84 100 46 102 32 86C26 80 25 70 24 58Z";

  /** One petal, a teardrop pointing up from (60,48); rotated around the centre. */
  const petal = (deg: number): Stroke => ({
    d: "M60 48C52 44 52 34 60 28C68 34 68 44 60 48Z",
    transform: `rotate(${deg} 60 48)`,
  });

  const drawings: Record<IllustrationName, Drawing> = {
    // A sprout with two leaves coming out of a little mound of soil.
    seedling: {
      wash: circle(66, 64, 40),
      tint: "sage",
      strokes: [
        "M22 94C38 89 50 96 62 92C74 88 88 96 100 91",
        "M61 92C60 80 61 68 62 54",
        "M61 68C46 68 35 60 33 45C48 45 58 54 61 68Z",
        "M60 66C54 58 46 52 38 48",
        "M62 56C67 40 81 31 97 32C97 47 84 58 62 56Z",
        "M64 54C72 46 82 40 92 35",
        "M38 101h5",
        "M56 103h8",
        "M76 101h4",
      ],
    },
    // A flower pressed under a taped sheet, petals flat.
    "pressed-flower": {
      wash: roundRect(46, 34, 62, 68, 14),
      tint: "rose",
      strokes: [
        "M30 24L88 21L91 97L28 99Z",
        "M46 17L68 15L69 26L47 28Z",
        "M60 90C58 78 60 68 60 55",
        "M60 80C51 79 45 73 45 66C53 66 59 72 60 80Z",
        petal(0),
        petal(72),
        petal(144),
        petal(216),
        petal(288),
        circle(60, 48, 3),
      ],
    },
    // A leaf, enlarged: the "nothing found" illustration.
    "magnifier-leaf": {
      wash: BLOB,
      tint: "amber",
      strokes: [
        "M28 82C22 54 42 34 68 30C70 56 56 80 28 82Z",
        "M28 82C40 68 52 52 64 36",
        "M43 65L35 60",
        "M52 53L44 46",
        "M56 70L64 66",
        circle(78, 72, 22),
        "M65 66C67 60 72 56 78 55",
        "M94 88L109 103L104 108L89 93",
      ],
    },
    // A can tipping water onto something that will grow.
    "watering-can": {
      wash: circle(58, 66, 42),
      tint: "sky",
      strokes: [
        "M32 56H78V88C78 92 75 95 71 95H39C35 95 32 92 32 88Z",
        "M40 56C40 42 70 42 70 56",
        "M32 64C16 64 16 86 32 86",
        "M78 72L100 48",
        "M78 84L105 56",
        "M98 44L110 58",
        "M104 68V73",
        "M111 64V70",
        "M98 72V76",
      ],
    },
    // A sleeping pot: nothing planted yet.
    "empty-pot": {
      wash: roundRect(24, 38, 70, 66, 16),
      tint: "clay",
      strokes: [
        roundRect(32, 60, 56, 12, 3),
        "M37 72L43 100H77L83 72",
        "M49 85Q52 88 55 85",
        "M65 85Q68 88 71 85",
        "M57 91Q60 93 63 91",
        "M60 34C57 34 55 38 56 42C57 45 62 45 64 42C65 38 63 34 60 34Z",
        "M60 48V53",
        "M60 56V58",
      ],
    },
    // A book with a shoot growing out of the spine.
    "sprouting-book": {
      wash: BLOB,
      tint: "teal",
      strokes: [
        "M60 97C48 91 34 91 22 95V52C34 48 48 48 60 54Z",
        "M60 97C72 91 86 91 98 95V52C86 48 72 48 60 54Z",
        "M30 62C36 60 44 60 51 62",
        "M30 71C36 69 44 69 51 71",
        "M69 62C76 60 84 60 90 62",
        "M60 78C60 66 60 54 60 42",
        "M60 52C50 52 44 46 44 37C54 37 60 43 60 52Z",
        "M60 44C62 33 70 27 80 27C80 38 72 44 60 44Z",
      ],
    },
  };
</script>

<script lang="ts">
  let {
    name,
    size = 120,
    animate = false,
    decorative = true,
    label,
  }: {
    name: IllustrationName;
    size?: number;
    /** Draw the strokes in once on mount. */
    animate?: boolean;
    /** Hidden from assistive tech unless given a `label`. */
    decorative?: boolean;
    label?: string;
  } = $props();

  const drawing = $derived(drawings[name]);
  const hidden = $derived(decorative && !label);
</script>

<svg
  class="illustration"
  class:animate
  width={size}
  height={size}
  viewBox="0 0 120 120"
  fill="none"
  focusable="false"
  aria-hidden={hidden ? "true" : undefined}
  role={hidden ? undefined : "img"}
  aria-label={hidden ? undefined : label}
  style:--tint="var(--c-{drawing.tint}-soft)"
>
  <path class="wash" d={drawing.wash} />
  <g class="ink">
    {#each drawing.strokes as stroke, i (i)}
      {#if typeof stroke === "string"}
        <path class="stroke" d={stroke} pathLength="1" style:--i={i} />
      {:else}
        <path class="stroke" d={stroke.d} transform={stroke.transform} pathLength="1" style:--i={i} />
      {/if}
    {/each}
  </g>
</svg>

<style>
  .illustration {
    display: block;
    flex: none;
    overflow: visible;
    color: var(--ill-ink, var(--text-soft));
  }
  .wash {
    fill: var(--tint);
    stroke: none;
  }
  .ink {
    stroke: currentColor;
    stroke-width: 1.4;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .stroke {
    fill: none;
  }

  .animate .wash {
    transform-box: fill-box;
    transform-origin: center;
    animation: wash-in var(--t-slow) var(--ease-out) backwards;
  }
  .animate .stroke {
    stroke-dasharray: 1 1;
    animation: draw 900ms var(--ease-out) backwards;
    /* Strokes follow one another; the stagger is capped so a drawing finishes in about a second. */
    animation-delay: calc(min(var(--i), 6) * 30ms);
  }
  @keyframes draw {
    from {
      stroke-dashoffset: 1;
      opacity: 0;
    }
    8% {
      opacity: 1;
    }
    to {
      stroke-dashoffset: 0;
      opacity: 1;
    }
  }
  @keyframes wash-in {
    from {
      opacity: 0;
      transform: scale(0.9);
    }
  }
</style>
