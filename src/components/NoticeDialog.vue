<script setup lang="ts">
import { useI18n } from "vue-i18n";

const { t } = useI18n();
defineProps<{ title: string; message?: string; messagePrefix?: string; messageSuffix?: string; kind?: "info" | "error"; linkLabel?: string; actionLabel?: string; closeLabel?: string }>();
defineEmits<{ close: []; openLink: []; action: [] }>();
</script>

<template>
  <div class="modal-backdrop">
    <section class="modal update-notice-dialog" role="alertdialog" aria-modal="true" :aria-label="title">
      <div class="modal-header"><div><span class="update-notice-icon" :class="kind ?? 'info'">{{ kind === "error" ? "!" : "i" }}</span><h2>{{ title }}</h2></div><button type="button" class="icon-button" :aria-label="t('common.close')" @click="$emit('close')">×</button></div>
      <div class="update-notice-body">
        <span v-if="linkLabel && messagePrefix && messageSuffix">{{ messagePrefix }} <button type="button" class="update-link" @click="$emit('openLink')">{{ linkLabel }}</button> {{ messageSuffix }}</span>
        <template v-else><span>{{ message }}</span><button v-if="linkLabel" type="button" class="update-link" @click="$emit('openLink')">{{ linkLabel }}</button></template>
      </div>
      <div class="modal-footer"><span></span><div><button type="button" class="secondary-button" @click="$emit('close')">{{ closeLabel ?? t('common.ok') }}</button><button v-if="actionLabel" class="primary-button" type="button" @click="$emit('action')">{{ actionLabel }}</button></div></div>
    </section>
  </div>
</template>
