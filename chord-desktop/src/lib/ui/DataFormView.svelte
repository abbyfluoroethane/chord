<script lang="ts">
  // A generic form renderer for XEP-0004 data forms: room configuration, ad-hoc commands,
  // registration and CAPTCHA. It shows every field type and changes `form.fields[i].values`
  // in place (bind:form). The caller sends `form` back. The rules are in forms.ts.
  import type { DataForm } from '$lib/chord/types';
  import Toggle from './Toggle.svelte';
  import {
    boolValue,
    fieldName,
    fieldProblem,
    firstValue,
    fromMultiline,
    inlineImage,
    isOn,
    optionText,
    otherMedia,
    selectedOption,
    toMultiline,
    toggleValue
  } from './forms';

  let {
    form = $bindable(),
    disabled = false,
    showTitle = true,
    showProblems = false,
    idPrefix = 'form'
  }: {
    form: DataForm;
    disabled?: boolean;
    /** Show the title and the instructions of the form above the fields. */
    showTitle?: boolean;
    /** Show what is wrong under each field. Turn it on after a failed send. */
    showProblems?: boolean;
    /** Keeps the element ids apart when two forms are on the page. */
    idPrefix?: string;
  } = $props();

  function set(i: number, values: string[]) {
    form.fields[i].values = values;
  }
</script>

<div class="form">
  {#if showTitle && form.title}<h3 class="title">{form.title}</h3>{/if}
  {#if showTitle && form.instructions}<p class="instructions">{form.instructions}</p>{/if}

  {#each form.fields as f, i (i)}
    {@const id = `${idPrefix}-${i}`}
    {@const problem = showProblems ? fieldProblem(f) : null}
    {#if f.kind === 'hidden'}
      <!-- Data for the server. The user never sees it. -->
    {:else if f.kind === 'fixed'}
      <p class="fixed">{f.values.join('\n')}</p>
    {:else if f.kind === 'boolean'}
      <div class="bool">
        <div class="text">
          <span id="{id}-l" class="name">{fieldName(f)}</span>
          {#if f.desc}<span class="meta">{f.desc}</span>{/if}
        </div>
        <Toggle
          checked={isOn(f)}
          label={fieldName(f)}
          {disabled}
          onchange={(on) => set(i, boolValue(on))}
        />
      </div>
    {:else}
      <div class="field" class:bad={!!problem}>
        {#if f.kind === 'list-multi'}
          <fieldset aria-describedby={problem ? `${id}-p` : undefined} {disabled}>
            <legend class="field-label">{fieldName(f)}{f.required ? ' *' : ''}</legend>
            {#each f.options as o, j (j)}
              <label class="check">
                <input
                  type="checkbox"
                  checked={f.values.includes(o.value)}
                  onchange={(e) => set(i, toggleValue(f, o.value, e.currentTarget.checked))}
                />
                {optionText(o)}
              </label>
            {/each}
          </fieldset>
        {:else}
          <label for={id}>{fieldName(f)}{f.required ? ' *' : ''}</label>
          {#if f.kind === 'list-single'}
            <select
              {id}
              class="input"
              {disabled}
              required={f.required}
              aria-invalid={!!problem}
              aria-describedby={problem ? `${id}-p` : undefined}
              value={selectedOption(f)}
              onchange={(e) => set(i, e.currentTarget.value === '' ? [] : [e.currentTarget.value])}
            >
              {#if selectedOption(f) === ''}<option value="">Choose…</option>{/if}
              {#each f.options as o, j (j)}
                <option value={o.value}>{optionText(o)}</option>
              {/each}
            </select>
          {:else if f.kind === 'text-multi' || f.kind === 'jid-multi'}
            <textarea
              {id}
              class="input area"
              rows="3"
              {disabled}
              spellcheck="false"
              aria-invalid={!!problem}
              aria-describedby={problem ? `${id}-p` : undefined}
              value={toMultiline(f.values)}
              oninput={(e) => set(i, fromMultiline(e.currentTarget.value))}
            ></textarea>
            {#if f.kind === 'jid-multi'}<span class="meta">One address on each line.</span>{/if}
          {:else}
            <input
              {id}
              class="input"
              class:mono={f.kind === 'jid-single'}
              type={f.kind === 'text-private' ? 'password' : 'text'}
              autocomplete={f.kind === 'text-private' ? 'new-password' : 'off'}
              autocapitalize="off"
              spellcheck="false"
              {disabled}
              aria-invalid={!!problem}
              aria-describedby={problem ? `${id}-p` : undefined}
              value={firstValue(f)}
              oninput={(e) => set(i, [e.currentTarget.value])}
            />
          {/if}
        {/if}

        {#each f.media as m, j (j)}
          {@const src = inlineImage(m)}
          {#if src}
            <img
              class="media"
              {src}
              alt="Image for {fieldName(f)}"
              width={m.width ?? undefined}
              height={m.height ?? undefined}
            />
          {/if}
        {/each}
        {#if otherMedia(f).length > 0}
          <span class="meta">This field has media that Chord does not load.</span>
        {/if}
        {#if f.desc}<span class="meta">{f.desc}</span>{/if}
        {#if problem}<span id="{id}-p" class="err" role="alert">{problem}</span>{/if}
      </div>
    {/if}
  {/each}
</div>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
  .title {
    margin: 0;
    font-size: 16px;
  }
  .instructions,
  .fixed {
    margin: 0;
    white-space: pre-line;
    color: var(--ink-muted);
  }
  .fixed {
    font-weight: 600;
    color: var(--ink);
  }
  .bool {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-4);
  }
  .text {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .name {
    font-weight: 500;
  }
  fieldset {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    margin: 0;
    padding: 0;
    border: 0;
  }
  .check {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .check input {
    width: 16px;
    height: 16px;
    accent-color: var(--brand);
  }
  .area {
    height: auto;
    padding: var(--space-2) var(--space-3);
    resize: vertical;
  }
  .media {
    align-self: flex-start;
    max-width: 100%;
    border-radius: var(--radius-md);
    background: #fff;
  }
  .err {
    color: var(--danger);
    font-size: 14px;
  }
  .bad :global(.input) {
    border-color: var(--danger);
  }
</style>
