resource "google_compute_network" "main" {
  name                    = "main"
  auto_create_subnetworks = false
}
