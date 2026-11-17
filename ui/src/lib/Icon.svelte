<script lang="ts">
  const paths = {
    send: "M4 12l15-7-5 15-3-6z",
    search: "M11 4a7 7 0 1 0 0 14 7 7 0 0 0 0-14zM16.5 16.5L20 20",
    group: "M9 11a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM3.5 19a5.5 5.5 0 0 1 11 0M16 5.2a3 3 0 0 1 0 5.6M17.5 14.2A5.5 5.5 0 0 1 20.5 19",
    image: "M5 5h14v14H5zM5 15l4-4 4 4 2.5-2.5L19 16M14.5 9.5h.01",
    doc: "M7 4h7l4 4v12H7zM14 4v4h4M10 13h5M10 16h5",
    mic: "M12 4a2.5 2.5 0 0 0-2.5 2.5v5a2.5 2.5 0 0 0 5 0v-5A2.5 2.5 0 0 0 12 4zM6.5 11.5a5.5 5.5 0 0 0 11 0M12 17v3",
    spark: "M12 4l1.8 5.2L19 11l-5.2 1.8L12 18l-1.8-5.2L5 11l5.2-1.8z",
    gear: "M12 9a3 3 0 1 0 0 6 3 3 0 0 0 0-6zM12 3v3M12 18v3M3 12h3M18 12h3M5.6 5.6l2.1 2.1M16.3 16.3l2.1 2.1M5.6 18.4l2.1-2.1M16.3 7.7l2.1-2.1",
    close: "M6 6l12 12M18 6L6 18",
    clip: "M19 11.5l-7 7a4.5 4.5 0 0 1-6.4-6.4l7.5-7.5a3 3 0 0 1 4.3 4.3l-7.5 7.4a1.5 1.5 0 0 1-2.1-2.1l6.7-6.7",
    check: "M5 12.5l4.5 4.5L19 7.5",
    globe: "M12 4a8 8 0 1 0 0 16 8 8 0 0 0 0-16zM4 12h16M12 4c2.5 2.5 2.5 13.5 0 16M12 4c-2.5 2.5-2.5 13.5 0 16",
    stop: "M7 7h10v10H7z",
    download: "M12 4v11M7.5 11l4.5 4.5 4.5-4.5M5 19h14",
    power: "M12 4v8M7.2 7.2a7 7 0 1 0 9.6 0",
  } as const;

  let { name, size = 20 }: { name: keyof typeof paths; size?: number } = $props();
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="1.6"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
>
  <path d={paths[name]} />
</svg>
