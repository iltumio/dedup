<script lang="ts">
	import '../app.css';
	import { app } from '$lib/state/app.svelte';

	let { children } = $props();

	$effect(() => {
		app.loadWorkspaces();
		app.loadCustomScanRules();
	});
</script>

{@render children()}
