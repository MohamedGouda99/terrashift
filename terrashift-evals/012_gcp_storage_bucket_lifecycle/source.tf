resource "aws_s3_bucket" "logs" {
  bucket        = "terrashift-logs"
  force_destroy = false
}
