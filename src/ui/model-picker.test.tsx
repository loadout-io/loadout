import { renderToStaticMarkup } from 'react-dom/server';
import { expect, it } from 'vitest';
import { ModelPicker } from './model-picker';
const choice = {
  id: 'new-model',
  displayName: 'New model',
  description: 'Fresh from the app',
  isDefault: true,
  hidden: false,
  aliases: [] as string[],
  efforts: ['high'],
};
const catalog = {
  models: [
    {
      id: 'new-model',
      displayName: 'New model',
      description: 'Fresh from the app',
      isDefault: true,
      hidden: false,
      aliases: [],
      efforts: ['high'],
    },
  ],
};
it('offers new vendor entries and names an invalid selection without changing it', () => {
  const html = renderToStaticMarkup(
    <ModelPicker
      id="test"
      vendor="codex"
      value="gpt-6"
      onChange={() => {
        throw new Error('must not silently replace');
      }}
      initialCatalog={catalog}
    />,
  );
  expect(html).toContain('New model · Recommended');
  expect(html).toContain('gpt-6 is not offered by Codex');
  expect(html).toContain('value="gpt-6"');
  expect(html).toContain('aria-invalid="true"');
});
it('recognizes a published alias and hides hidden choices', () => {
  const html = renderToStaticMarkup(
    <ModelPicker
      id="test"
      vendor="codex"
      value="alias"
      onChange={() => {}}
      initialCatalog={{
        models: [
          { ...choice, aliases: ['alias'] },
          { ...choice, id: 'internal', hidden: true },
        ],
      }}
    />,
  );
  expect(html).toContain('aria-invalid="false"');
  expect(html).not.toContain('value="internal"');
});
