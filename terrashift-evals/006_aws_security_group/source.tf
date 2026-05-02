resource "google_compute_firewall" "web" {
  name    = "allow-web"
  network = google_compute_network.main.self_link

  allow {
    protocol = "tcp"
    ports    = ["80", "443"]
  }
}
