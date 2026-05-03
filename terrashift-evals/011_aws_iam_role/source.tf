resource "google_service_account" "migrator" {
  account_id   = "terrashift-migrator"
  display_name = "Terrashift Migrator"
  description  = "Service account that runs Terrashift migrations"
}
