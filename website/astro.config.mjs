// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';
import starlightLinksValidator from 'starlight-links-validator';
import { visit } from 'unist-util-visit';

// Astro doesn't auto-prefix root-relative markdown links with `base`. This
// remark plugin walks every link node and prepends BASE if it starts with /
// and isn't already prefixed. External URLs and anchors are left alone.
function remarkBasePrefix() {
  const base = process.env.BASE || '';
  if (!base) return () => {};
  const prefix = base.endsWith('/') ? base.slice(0, -1) : base;
  return () => (tree) => {
    visit(tree, 'link', (node) => {
      const url = node.url || '';
      if (
        url.startsWith('/') &&
        !url.startsWith('//') &&
        !url.startsWith(prefix + '/') &&
        url !== prefix
      ) {
        node.url = prefix + url;
      }
    });
  };
}

// site + base are injected by the GH Pages workflow for production builds via
// SITE / BASE env vars. Leaving them unset keeps local dev / CI builds simple.
export default defineConfig({
  ...(process.env.SITE ? { site: process.env.SITE } : {}),
  ...(process.env.BASE ? { base: process.env.BASE } : {}),
  markdown: {
    remarkPlugins: [remarkBasePrefix()],
  },
  integrations: [
    starlight({
      title: 'Picodroid',
      description:
        'A stripped-down, FreeRTOS-based version of Android for the Raspberry Pi Pico.',
      logo: { src: './src/assets/picodroid.svg', replacesTitle: true },
      customCss: ['./src/styles/custom.css'],
      social: {
        github: 'https://github.com/shivrajora/picodroid-rs',
      },
      // Run the links validator only when BASE isn't set: with a non-empty
      // base path, the validator reports false positives because internal
      // links are written without the prefix and the plugin doesn't apply
      // the base before checking. Local / CI builds without BASE still
      // catch real broken links.
      plugins: process.env.BASE ? [] : [starlightLinksValidator()],
      sidebar: [
        { label: 'Overview', link: '/' },
        {
          label: 'Get started',
          items: [
            { label: 'Build & flash (RP)', slug: 'get-started/build' },
            { label: 'ESP32-S3 quickstart', slug: 'get-started/esp32s3' },
            { label: 'Hot-swap with pdb', slug: 'get-started/hot-swap' },
            { label: 'Host simulator', slug: 'get-started/simulator' },
            { label: 'Your first app', slug: 'get-started/first-app' },
          ],
        },
        {
          label: 'Tutorials',
          items: [
            { label: 'Multi-screen app', slug: 'tutorials/multi-screen-app' },
            { label: 'Background service', slug: 'tutorials/background-service' },
          ],
        },
        { label: 'Examples', slug: 'examples' },
        {
          label: 'Java API',
          items: [
            { label: 'Overview', slug: 'api' },
            { label: 'Core language', slug: 'api/core' },
            { label: 'System & concurrency', slug: 'api/system' },
            { label: 'Services & DI', slug: 'api/services', badge: 'Preview' },
            { label: 'Peripherals', slug: 'api/peripherals' },
            { label: 'Storage', slug: 'api/storage' },
            { label: 'Networking', slug: 'api/networking' },
            { label: 'Sensors', slug: 'api/sensors' },
            { label: 'Graphics & UI', slug: 'api/ui' },
          ],
        },
        {
          label: 'Guides',
          items: [
            { label: 'Embedded gotchas', slug: 'guides/embedded-gotchas' },
            { label: 'Button-only navigation', slug: 'guides/button-navigation' },
            { label: 'Debugging', slug: 'guides/debugging' },
            { label: 'Troubleshooting', slug: 'guides/troubleshooting' },
            { label: 'Bundled image assets', slug: 'guides/assets' },
            { label: 'Theming', slug: 'guides/theming' },
          ],
        },
        {
          label: 'Reference',
          items: [
            { label: 'Limits & memory budgets', slug: 'reference/limits' },
            { label: 'Manifest', slug: 'reference/manifest' },
            { label: 'Cargo aliases', slug: 'reference/cargo-aliases' },
            { label: 'Class-name shrinker', slug: 'reference/shrinker' },
            { label: 'Advanced configuration', slug: 'reference/advanced-config' },
            { label: 'JVM tunables', slug: 'reference/jvm-tunables' },
            { label: 'Porting guide', slug: 'reference/porting-guide' },
            { label: 'RP2350 SMP bugs', slug: 'reference/rp2350-freertos-smp-bugs' },
            { label: 'ESP32-S3 toolchain', slug: 'reference/esp32s3-toolchain' },
          ],
        },
        {
          label: 'Project',
          items: [
            { label: 'Architecture', slug: 'project/architecture' },
            { label: 'Release notes', slug: 'project/release-notes' },
            { label: 'Contributing', slug: 'project/contributing' },
            { label: 'Licensing', slug: 'project/licensing' },
            { label: 'CLA', slug: 'project/cla' },
          ],
        },
      ],
    }),
  ],
});
