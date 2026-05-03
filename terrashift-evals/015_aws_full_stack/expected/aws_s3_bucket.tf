resource "aws_s3_bucket" "artifacts" {
  bucket = "myorg-artifacts"
  acl = "private"
}

resource "aws_s3_bucket" "assets" {
  bucket = "myorg-assets"
  acl = "private"
}

resource "aws_s3_bucket" "backups" {
  bucket = "myorg-backups"
  acl = "private"
}

resource "aws_s3_bucket" "logs" {
  bucket = "myorg-logs"
  acl = "private"
}

resource "aws_s3_bucket" "uploads" {
  bucket = "myorg-uploads"
  acl = "private"
}
