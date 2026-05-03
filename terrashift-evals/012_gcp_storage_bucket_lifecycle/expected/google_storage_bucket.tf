resource "google_storage_bucket" "logs" {
  name = "terrashift-logs"
  location = "US"
  force_destroy = false
  storage_class = "STANDARD"
}
