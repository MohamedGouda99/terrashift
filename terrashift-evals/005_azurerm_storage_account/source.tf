resource "aws_s3_bucket" "assets" {
  bucket = "myorg-assets"
  acl    = "private"
}
