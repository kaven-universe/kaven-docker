<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

const props = defineProps<{ version: string; phase: "checking" | "downloading" | "installing"; downloaded: number; total?: number }>();
const { t } = useI18n();
const percentage = computed(() => props.total ? Math.min(100, Math.round(props.downloaded / props.total * 100)) : undefined);
const status = computed(() => {
  if (props.phase === "checking") return t("update.preparing");
  if (props.phase === "installing") return t("update.startingInstaller");
  return percentage.value === undefined ? t("update.downloading") : t("update.downloadingPercent", { percentage: percentage.value });
});
</script>

<template>
  <div class="modal-backdrop update-progress-backdrop">
    <section class="modal update-progress-dialog" role="dialog" aria-modal="true" :aria-label="t('update.installing')">
      <div class="modal-header"><div><span class="update-notice-icon">↓</span><div><h2>{{ t("update.installingVersion", { version }) }}</h2><p>{{ t("update.signatureHint") }}</p></div></div></div>
      <div class="update-progress-body">
        <strong>{{ status }}</strong>
        <progress v-if="phase === 'downloading' && total" :value="downloaded" :max="total" />
        <div v-else class="update-progress-indeterminate"><span></span></div>
        <small v-if="phase === 'downloading' && total">{{ Math.min(downloaded, total).toLocaleString() }} / {{ total.toLocaleString() }} bytes</small>
      </div>
      <div class="modal-footer"><span>{{ t("update.doNotClose") }}</span></div>
    </section>
  </div>
</template>
